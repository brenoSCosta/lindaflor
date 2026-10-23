#!/usr/bin/env bash
# Logical Postgres backup: `pg_dump --format=custom`, local rotation, optional
# off-site copy to S3.
#
# Usage: scripts/backup.sh [label]
#   label  free-form tag in the file name (default: "scheduled"; the migrate
#          job uses "pre-migrate").
#
# Environment:
#   DATABASE_URL / DATABASE_URL_FILE   connection string (required; *_FILE is
#                                      the Docker-secret form, direct env wins)
#   BACKUP_DIR            output directory (default: ./backups)
#   BACKUP_KEEP_DAYS      delete local dumps older than N days (default: 14)
#   BACKUP_KEEP_MIN       always keep at least the newest N dumps (default: 7)
#   BACKUP_S3_URI         optional, e.g. s3://lindaflor-backups/postgres
#   BACKUP_S3_ENDPOINT    optional custom endpoint (RustFS/MinIO/R2)
#   AWS_ACCESS_KEY_ID(_FILE), AWS_SECRET_ACCESS_KEY(_FILE), AWS_DEFAULT_REGION
#                         credentials for the upload (`aws` CLI)
#
# Restore (into an empty database):
#   pg_restore --clean --if-exists --no-owner --dbname "$DATABASE_URL" <file>.dump
#
# This is a logical snapshot (RPO = time since the last dump). Point-in-time
# recovery needs WAL archiving on the server — see README "Deploy & backups".
set -euo pipefail

log() { printf '[backup] %s\n' "$*" >&2; }

# Read KEY, falling back to the file named by KEY_FILE (mirrors internal/config.ReadEnvOrFile).
load_secret() {
  local key="$1" file_key="${1}_FILE"
  if [ -z "${!key:-}" ] && [ -n "${!file_key:-}" ]; then
    if [ ! -r "${!file_key}" ]; then
      log "$file_key=${!file_key} is not readable"
      exit 1
    fi
    printf -v "$key" '%s' "$(cat "${!file_key}")"
    export "${key?}"
  fi
}

load_secret DATABASE_URL
load_secret AWS_ACCESS_KEY_ID
load_secret AWS_SECRET_ACCESS_KEY

if [ -z "${DATABASE_URL:-}" ]; then
  log "DATABASE_URL (or DATABASE_URL_FILE) is required"
  exit 1
fi

label="${1:-scheduled}"
case "$label" in
  *[!A-Za-z0-9_-]*) log "label must match [A-Za-z0-9_-]+: $label"; exit 1 ;;
esac

backup_dir="${BACKUP_DIR:-./backups}"
keep_days="${BACKUP_KEEP_DAYS:-14}"
keep_min="${BACKUP_KEEP_MIN:-7}"
mkdir -p "$backup_dir"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
file="$backup_dir/lindaflor-${stamp}-${label}.dump"
tmp="${file}.partial"
trap 'rm -f "$tmp"' EXIT

log "dumping to $file"
# Custom format: compressed, restorable selectively with pg_restore. No owner
# or ACL statements so a restore works into a differently-named role.
pg_dump --format=custom --compress=6 --no-owner --no-acl \
  --dbname="$DATABASE_URL" --file="$tmp"
# A dump that pg_restore cannot list is not a backup.
pg_restore --list "$tmp" >/dev/null
mv "$tmp" "$file"
trap - EXIT
log "wrote $(du -h "$file" | cut -f1) $file"

if [ -n "${BACKUP_S3_URI:-}" ]; then
  if ! command -v aws >/dev/null 2>&1; then
    log "BACKUP_S3_URI is set but the aws CLI is not installed"
    exit 1
  fi
  endpoint_args=()
  if [ -n "${BACKUP_S3_ENDPOINT:-}" ]; then
    endpoint_args=(--endpoint-url "$BACKUP_S3_ENDPOINT")
  fi
  dest="${BACKUP_S3_URI%/}/$(basename "$file")"
  log "uploading to $dest"
  aws "${endpoint_args[@]}" s3 cp --only-show-errors "$file" "$dest"
fi

# Rotation: drop dumps older than keep_days, but never the newest keep_min.
mapfile -t dumps < <(ls -1t "$backup_dir"/lindaflor-*.dump 2>/dev/null || true)
if [ "${#dumps[@]}" -gt "$keep_min" ]; then
  for old in "${dumps[@]:$keep_min}"; do
    if [ -n "$(find "$old" -maxdepth 0 -mtime +"$keep_days" 2>/dev/null)" ]; then
      log "rotating out $old"
      rm -f -- "$old"
    fi
  done
fi

printf '%s\n' "$file"
