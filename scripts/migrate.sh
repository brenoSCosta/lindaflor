#!/usr/bin/env bash
# Migrate job: back up, then apply pending goose migrations.
#
# Run this (once, single instance) before rolling out a web release. The web
# process never migrates in production (`MIGRATE_ON_BOOT` defaults off) and
# refuses to boot while migrations are pending, so replicas cannot race.
#
# Usage: scripts/migrate.sh            backup + migrate
#        scripts/migrate.sh backup     backup only (scheduled job)
#        scripts/migrate.sh info       show applied/pending migrations
#
# Environment (see scripts/backup.sh for the backup variables):
#   DATABASE_URL / DATABASE_URL_FILE   required
#   MIGRATIONS_DIR        default: <repo>/db/migrations (image: /migrations)
#   SKIP_PRE_MIGRATE_BACKUP=1          skip the backup (e.g. first deploy on
#                                      an empty database) — never routine.
#
# Migrations are forward-only (no `-- +goose Down` is ever run in
# production). If a migration breaks production, restore the pre-migrate
# dump or use PITR, then ship a fix.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
migrations_dir="${MIGRATIONS_DIR:-$script_dir/../db/migrations}"

log() { printf '[migrate] %s\n' "$*" >&2; }

if [ -z "${DATABASE_URL:-}" ] && [ -n "${DATABASE_URL_FILE:-}" ]; then
  if [ ! -r "$DATABASE_URL_FILE" ]; then
    log "DATABASE_URL_FILE=$DATABASE_URL_FILE is not readable"
    exit 1
  fi
  DATABASE_URL="$(cat "$DATABASE_URL_FILE")"
  export DATABASE_URL
fi
if [ -z "${DATABASE_URL:-}" ]; then
  log "DATABASE_URL (or DATABASE_URL_FILE) is required"
  exit 1
fi

goose_cmd() {
  # Same table goose's Go provider uses in internal/db (goose_db_version).
  goose -dir "$migrations_dir" postgres "$DATABASE_URL" "$@"
}

wait_for_db() {
  for _ in $(seq 1 30); do
    if pg_isready --dbname="$DATABASE_URL" --timeout=2 >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  log "database is not reachable"
  exit 1
}

command_name="${1:-migrate}"
case "$command_name" in
  backup)
    wait_for_db
    exec "$script_dir/backup.sh" "${2:-scheduled}"
    ;;
  info)
    wait_for_db
    goose_cmd status
    exit 0
    ;;
  migrate) ;;
  *)
    log "unknown command: $command_name (expected migrate, backup or info)"
    exit 2
    ;;
esac

wait_for_db

log "migration status before:"
goose_cmd status >&2

if [ "${SKIP_PRE_MIGRATE_BACKUP:-0}" = "1" ]; then
  log "SKIP_PRE_MIGRATE_BACKUP=1: not taking a pre-migrate backup"
else
  "$script_dir/backup.sh" pre-migrate >/dev/null
fi

log "applying migrations from $migrations_dir"
goose_cmd up
goose_cmd version
log "done"
