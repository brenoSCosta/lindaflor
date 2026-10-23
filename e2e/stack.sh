#!/usr/bin/env bash
# Boots a throwaway production-like stack from nix packages and runs the
# Playwright suite against it (see e2e/README.md). Invoked by `make e2e`,
# which provides the tools on PATH:
#   go, postgres (initdb/pg_ctl/psql), valkey-server   (nix develop)
#   mailpit, versitygw, caddy, node, playwright        (nix shell)
#
#   e2e/stack.sh [flow ...]     flows: admin customer fulfil guest jobs security prod
#
# Everything lives under $E2E_DIR (default e2e/.out) and is stopped on exit.
# E2E_KEEP=1 leaves the stack running for debugging.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${E2E_DIR:-$ROOT/e2e/.out}
P=${E2E_PORT_BASE:-4740}
PG_PORT=$((P + 1)) VALKEY_PORT=$((P + 2)) S3_PORT=$((P + 3)) SMTP_PORT=$((P + 5)) MAILPIT_PORT=$((P + 6))
FAKEMP_PORT=$((P + 7)) APP_PORT=$((P + 10)) TLS_PORT=$((P + 11)) PROD_PORT=$((P + 12)) S3_TLS_PORT=$((P + 13))

FLOWS=("$@")
[ ${#FLOWS[@]} -eq 0 ] && FLOWS=(admin customer fulfil guest jobs security prod)

for tool in go initdb pg_ctl psql valkey-server mailpit versitygw node; do
	command -v "$tool" >/dev/null || {
		echo "missing $tool on PATH (run via \`make e2e\`)" >&2
		exit 1
	}
done

rm -rf "$OUT"
mkdir -p "$OUT"/{bin,log,s3,shots}
PIDS=()
# shellcheck disable=SC2329 # invoked by the EXIT trap
cleanup() {
	if [ "${E2E_KEEP:-}" = 1 ]; then
		echo "E2E_KEEP=1: stack left running (app http://127.0.0.1:$APP_PORT, mailpit http://127.0.0.1:$MAILPIT_PORT)"
		return
	fi
	for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
	pg_ctl -D "$OUT/pg" -m fast stop >/dev/null 2>&1 || true
}
trap cleanup EXIT
bg() { # bg <log> <cmd...>
	local log=$1
	shift
	"$@" >"$OUT/log/$log" 2>&1 &
	PIDS+=($!)
}
wait_http() { # wait_http <url> [curl args]
	local url=$1
	shift
	for _ in $(seq 120); do
		curl -sf -o /dev/null "$@" "$url" && return 0
		sleep 0.5
	done
	echo "timed out waiting for $url" >&2
	return 1
}

echo "== building server, seed, create-admin, fakemp"
(cd "$ROOT" && CGO_ENABLED=0 go build -o "$OUT/bin/" ./cmd/server ./cmd/seed ./cmd/create-admin)
(cd "$ROOT/e2e/fakemp" && CGO_ENABLED=0 go build -o "$OUT/bin/fakemp" .)

echo "== postgres :$PG_PORT, valkey :$VALKEY_PORT, s3 :$S3_PORT, mailpit :$SMTP_PORT/:$MAILPIT_PORT, fake MP :$FAKEMP_PORT"
initdb -D "$OUT/pg" -U postgres --auth=trust >"$OUT/log/initdb.log"
pg_ctl -D "$OUT/pg" -o "-p $PG_PORT -k '' -c listen_addresses=127.0.0.1" -l "$OUT/log/postgres.log" -w start >/dev/null
createdb -h 127.0.0.1 -p "$PG_PORT" -U postgres lindaflor
bg valkey.log valkey-server --port "$VALKEY_PORT" --bind 127.0.0.1 --save '' --appendonly no
bg s3.log versitygw --port "127.0.0.1:$S3_PORT" --access e2eaccess --secret e2esecret-0123456789 posix "$OUT/s3"
bg mailpit.log mailpit --smtp "127.0.0.1:$SMTP_PORT" --listen "127.0.0.1:$MAILPIT_PORT" --smtp-auth-accept-any --smtp-auth-allow-insecure
MP_ACCESS_TOKEN=TEST-e2e-token MP_WEBHOOK_SECRET=e2e-webhook-secret bg fakemp.log "$OUT/bin/fakemp" -addr "127.0.0.1:$FAKEMP_PORT"
wait_http "http://127.0.0.1:$MAILPIT_PORT/api/v1/messages"
wait_http "http://127.0.0.1:$FAKEMP_PORT/_fake/payments"

# App environment (development mode: MP_API_BASE_URL is plain http).
export DATABASE_URL="postgres://postgres@127.0.0.1:$PG_PORT/lindaflor?sslmode=disable"
export VALKEY_URL="redis://127.0.0.1:$VALKEY_PORT"
export S3_ENDPOINT="http://127.0.0.1:$S3_PORT" S3_ACCESS_KEY_ID=e2eaccess S3_SECRET_ACCESS_KEY=e2esecret-0123456789 S3_BUCKET=lindaflor S3_REGION=us-east-1
export MAIL_TRANSPORT=smtp SMTP_HOST=127.0.0.1 SMTP_PORT=$SMTP_PORT SMTP_TLS=none MAIL_FROM='Lindaflor <loja@lindaflor.test>' MAIL_POLL_SECS=1
export PAYMENT_PROVIDER=mercadopago MP_API_BASE_URL="http://127.0.0.1:$FAKEMP_PORT" MP_ACCESS_TOKEN=TEST-e2e-token MP_WEBHOOK_SECRET=e2e-webhook-secret
export HOST=127.0.0.1 PORT=$APP_PORT APP_ENV=development APP_ORIGIN="http://127.0.0.1:$APP_PORT" MIGRATE_ON_BOOT=1
export TOKEN_PEPPER=e2e-pepper-0123456789abcdef0123456789abcdef
TOTP_ENCRYPTION_KEY=$(head -c 32 /dev/urandom | base64)
export TOTP_ENCRYPTION_KEY
export RESERVATION_SWEEP_SECS=5 ABANDONED_CART_AFTER_MINUTES=1 LOG_SAMPLE_RATE=1
export SEED_ADMIN_EMAIL=admin@lindaflor.test SEED_ADMIN_PASSWORD='Adm1n-Sup3r-Secret!' SEED_ADMIN_NAME=Admin SEED_PRODUCT_COUNT=24 SEED_SET_COUNT=4

echo "== app :$APP_PORT (development, Mercado Pago → fake)"
bg app.log "$OUT/bin/server"
wait_http "http://127.0.0.1:$APP_PORT/api/readyz"
curl -s "http://127.0.0.1:$APP_PORT/api/readyz"
echo
"$OUT/bin/seed" >"$OUT/log/seed.log" 2>&1
printf '%s\n' 'Boss-Sup3r-Secret!x' | "$OUT/bin/create-admin" --email boss@lindaflor.test --name Boss >>"$OUT/log/seed.log" 2>&1

# Playwright: e2e/node_modules (npm ci) or the nix playwright-test package.
if [ -z "${PLAYWRIGHT_MODULE:-}" ]; then
	if [ -d "$ROOT/e2e/node_modules/playwright" ]; then
		PLAYWRIGHT_MODULE=playwright
	elif command -v playwright >/dev/null; then
		pw=$(command -v playwright)
		PLAYWRIGHT_MODULE="$(dirname "$(dirname "$(readlink -f "$pw")")")/lib/node_modules/playwright/index.mjs"
		PLAYWRIGHT_BROWSERS_PATH=${PLAYWRIGHT_BROWSERS_PATH:-$(grep -o "/nix/store/[^']*-playwright-browsers" "$pw" | head -1)}
		export PLAYWRIGHT_BROWSERS_PATH
	else
		echo "no Playwright: run \`npm ci\` in e2e/ or use \`make e2e\`" >&2
		exit 1
	fi
fi
export PLAYWRIGHT_MODULE
export E2E_MAILPIT_URL="http://127.0.0.1:$MAILPIT_PORT" E2E_FAKEMP_URL="http://127.0.0.1:$FAKEMP_PORT"
export E2E_SHOTS="$OUT/shots" E2E_STATE="$OUT/state.json"
export E2E_ADMIN_EMAIL=$SEED_ADMIN_EMAIL E2E_ADMIN_PASSWORD=$SEED_ADMIN_PASSWORD

status=0
dev=() prod=0
for f in "${FLOWS[@]}"; do
	if [ "$f" = prod ]; then prod=1; else dev+=("$f"); fi
done
if [ ${#dev[@]} -gt 0 ]; then
	echo "== flows: ${dev[*]}"
	E2E_BASE_URL="http://127.0.0.1:$APP_PORT" E2E_APP_LOG="$OUT/log/app.log" node "$ROOT/e2e/run.mjs" "${dev[@]}" || status=1
fi

if [ $prod = 1 ]; then
	command -v caddy >/dev/null || {
		echo "missing caddy on PATH" >&2
		exit 1
	}
	echo "== production smoke: app :$PROD_PORT behind caddy https://localhost:$TLS_PORT"
	cat >"$OUT/Caddyfile" <<-EOF
		{
			admin off
			auto_https disable_redirects
			skip_install_trust
			storage file_system $OUT/caddy
		}
		https://localhost:$TLS_PORT {
			tls internal
			reverse_proxy 127.0.0.1:$PROD_PORT
		}
		https://localhost:$S3_TLS_PORT {
			tls internal
			reverse_proxy 127.0.0.1:$S3_PORT
		}
	EOF
	bg caddy.log caddy run --config "$OUT/Caddyfile" --adapter caddyfile
	for _ in $(seq 60); do [ -f "$OUT/caddy/pki/authorities/local/root.crt" ] && break; sleep 0.5; done
	# Production refuses plain SMTP, http MP and http S3 links on an https
	# page: log mail, static PIX, S3 through the TLS proxy.
	(
		export APP_ENV=production PORT=$PROD_PORT APP_ORIGIN="https://localhost:$TLS_PORT" TRUSTED_PROXY_HOPS=1 MIGRATE_ON_BOOT=0
		export S3_ENDPOINT="https://localhost:$S3_TLS_PORT" SSL_CERT_FILE="$OUT/caddy/pki/authorities/local/root.crt"
		export MAIL_TRANSPORT=log MAIL_ALLOW_LOG_IN_PRODUCTION=1 PAYMENT_PROVIDER=static_pix ALLOW_STATIC_PIX=1
		export METRICS_TOKEN=e2e-metrics-token-0123456789abcdef
		unset MP_API_BASE_URL MP_ACCESS_TOKEN MP_WEBHOOK_SECRET
		exec "$OUT/bin/server"
	) >"$OUT/log/prod.log" 2>&1 &
	PIDS+=($!)
	wait_http "https://localhost:$TLS_PORT/api/readyz" -k
	E2E_BASE_URL="https://localhost:$TLS_PORT" E2E_IGNORE_HTTPS_ERRORS=1 E2E_APP_LOG="$OUT/log/prod.log" \
		node "$ROOT/e2e/run.mjs" prod || status=1
fi

echo "logs: $OUT/log  screenshots: $OUT/shots"
exit $status
