#!/usr/bin/env bash
# Migrate job: back up, then apply pending sqlx migrations.
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
#   MIGRATIONS_DIR        default: <repo>/migrations
#   SKIP_PRE_MIGRATE_BACKUP=1          skip the backup (e.g. first deploy on
#                                      an empty database) — never routine.
#
# There are no down-migrations (forward-fix only). If a migration breaks
# production, restore the pre-migrate dump or use PITR, then ship a fix.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
migrations_dir="${MIGRATIONS_DIR:-$script_dir/../migrations}"

log() { printf '[migrate] %s\n' "$*" >&2; }

if [ -z "${DATABASE_URL:-}" ] && [ -n "${DATABASE_URL_FILE:-}" ]; then
  DATABASE_URL="$(cat "$DATABASE_URL_FILE")"
  export DATABASE_URL
fi
if [ -z "${DATABASE_URL:-}" ]; then
  log "DATABASE_URL (or DATABASE_URL_FILE) is required"
  exit 1
fi

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
    exec sqlx migrate info --source "$migrations_dir"
    ;;
  migrate) ;;
  *)
    log "unknown command: $command_name (expected migrate, backup or info)"
    exit 2
    ;;
esac

wait_for_db

log "migration status before:"
sqlx migrate info --source "$migrations_dir" >&2

if [ "${SKIP_PRE_MIGRATE_BACKUP:-0}" = "1" ]; then
  log "SKIP_PRE_MIGRATE_BACKUP=1: not taking a pre-migrate backup"
else
  "$script_dir/backup.sh" pre-migrate >/dev/null
fi

log "applying migrations from $migrations_dir"
sqlx migrate run --source "$migrations_dir"
log "done"
