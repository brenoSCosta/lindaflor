#!/usr/bin/env bash
# Boot smoke test for the production images (CI `docker` job, or locally
# after `make docker`): Postgres 17 + Valkey, the migrate job (backup +
# goose up), then the web image, which must report ready on /api/readyz and
# turn healthy through its own HEALTHCHECK.
#
# Usage: scripts/docker-smoke.sh [web-image] [migrate-image]
#   defaults: lindaflor:ci lindaflor-migrate:ci
#   SMOKE_PORT  host port for the web container (default 14200)
set -euo pipefail

web_image="${1:-lindaflor:ci}"
migrate_image="${2:-lindaflor-migrate:ci}"
port="${SMOKE_PORT:-14200}"
id="lf-smoke-$$"
net="$id"
pg="$id-pg"
vk="$id-valkey"
web="$id-web"

log() { printf '[smoke] %s\n' "$*" >&2; }

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    log "failed; web logs:"
    docker logs "$web" 2>&1 | tail -n 50 >&2 || true
  fi
  docker rm -f "$web" "$pg" "$vk" >/dev/null 2>&1 || true
  docker network rm "$net" >/dev/null 2>&1 || true
  exit "$status"
}
trap cleanup EXIT

log "static checks"
uid="$(docker image inspect --format '{{.Config.User}}' "$web_image")"
[ "$uid" = "10001:10001" ] || { log "runtime user is '$uid', want 10001:10001"; exit 1; }
docker run --rm --entrypoint goose "$migrate_image" --version
docker run --rm --entrypoint pg_dump "$migrate_image" --version
docker run --rm --entrypoint aws "$migrate_image" --version

docker network create "$net" >/dev/null
docker run -d --name "$pg" --network "$net" \
  -e POSTGRES_PASSWORD=pg -e POSTGRES_DB=lindaflor postgres:17-bookworm >/dev/null
docker run -d --name "$vk" --network "$net" valkey/valkey:8-bookworm >/dev/null
for _ in $(seq 1 30); do
  docker exec "$pg" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1 && break
  sleep 2
done

db_url="postgres://postgres:pg@$pg:5432/lindaflor?sslmode=disable"

log "migrate job"
docker run --rm --network "$net" -e DATABASE_URL="$db_url" "$migrate_image"
docker run --rm --network "$net" -e DATABASE_URL="$db_url" "$migrate_image" info
log "backup job"
docker run --rm --network "$net" -e DATABASE_URL="$db_url" "$migrate_image" backup scheduled

log "web"
docker run -d --name "$web" --network "$net" -p "127.0.0.1:$port:4200" \
  --read-only --tmpfs /tmp --cap-drop ALL --security-opt no-new-privileges:true \
  -e APP_ORIGIN="http://localhost:$port" \
  -e DATABASE_URL="$db_url" \
  -e VALKEY_URL="redis://$vk:6379" \
  -e TOTP_ENCRYPTION_KEY="$(openssl rand -base64 32)" \
  -e TOKEN_PEPPER="$(openssl rand -hex 32)" \
  -e METRICS_TOKEN="$(openssl rand -hex 16)" \
  -e MAIL_TRANSPORT=log -e MAIL_ALLOW_LOG_IN_PRODUCTION=1 \
  -e MAIL_FROM="Lindaflor <ci@lindaflor.invalid>" \
  -e ALLOW_STATIC_PIX=1 \
  "$web_image" >/dev/null

ready=""
for _ in $(seq 1 30); do
  if ready="$(curl -fs "http://127.0.0.1:$port/api/readyz")"; then break; fi
  sleep 1
done
[ -n "$ready" ] || { log "/api/readyz never became ready"; exit 1; }
log "readyz: $ready"
curl -fsS "http://127.0.0.1:$port/api/health" >/dev/null
code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/")"
[ "$code" = "200" ] || { log "GET / returned $code"; exit 1; }
code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/metrics")"
[ "$code" = "401" ] || { log "GET /metrics without token returned $code, want 401"; exit 1; }

log "container HEALTHCHECK"
health=""
for _ in $(seq 1 30); do
  health="$(docker inspect --format '{{.State.Health.Status}}' "$web")"
  [ "$health" = "healthy" ] && break
  sleep 2
done
[ "$health" = "healthy" ] || { log "health status: $health"; exit 1; }
log "ok"
