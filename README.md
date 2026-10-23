# Lindaflor

Brazilian swimwear storefront (UI copy in pt-BR): a server-rendered Go web
app — storefront, cart and checkout with PIX (static BR Code or Mercado
Pago), customer account, admin area, and a JSON auth API (`/api/auth/*`,
OpenAPI in development).

- **App**: Go 1.27 · [chi](https://go-chi.io) router · [templ](https://templ.guide) components · [htmx 4](https://four.htmx.org) + [Alpine.js](https://alpinejs.dev) · [Tailwind CSS v4](https://tailwindcss.com)
- **Data**: PostgreSQL 17 via [pgx](https://github.com/jackc/pgx) + [sqlc](https://sqlc.dev) (generated, type-safe queries) · [goose](https://pressly.github.io/goose/) migrations
- **Sessions / rate limits / lockouts**: [Valkey](https://valkey.io) (Redis-compatible) via go-redis, with an in-process fallback
- **Object storage**: S3 API ([RustFS](https://rustfs.com) locally) for product images and avatars
- **Environment**: Nix flake + [devenv](https://devenv.sh) 2.3 or newer for the toolchain, Postgres, Valkey and RustFS

> Lindaflor was migrated from a Rust/Topcoat implementation, which has been
> removed (it lives in git history); see
> [`docs/GO_MIGRATION.md`](docs/GO_MIGRATION.md) for the migration record and
> the intentional behavior differences.

## Quick start

Requires Nix with flakes and [devenv](https://devenv.sh) 2.3+ (earlier
versions can leave Postgres stuck in `stopping` on cancel). Docker is used
for the RustFS container.

```bash
direnv allow        # first time: auto-activates the devenv shell (or run `devenv shell`)
devenv up           # postgres, valkey, rustfs, pgweb + the app under air (live reload)
make seed           # admin user + demo catalog (idempotent)
```

Open http://localhost:4200 and sign in as `admin@lindaflor.com` /
`admin123` (dev default). In development the server applies pending
migrations on boot; `make migrate` does it explicitly.

Without devenv, `nix develop` gives the same toolchain (go 1.27.3, templ,
sqlc, goose, golangci-lint v2, gofumpt, goimports, gopls, air,
tailwindcss 4, postgres 17, valkey, govulncheck, hadolint, actionlint,
shellcheck); start Postgres/Valkey yourself and export the variables below.

### Ports

| Port | Service |
| --- | --- |
| 4200 | App (`http://localhost:4200`) |
| 4201 | PostgreSQL 17 (db `lindaflor`, user/password `postgres`) |
| 4202 | Valkey |
| 4203 | RustFS S3 API |
| 4204 | RustFS console (`rustfsadmin` / `rustfsadmin`) |
| 4205 | pgweb (Postgres browser) |

`devenv shell ports` prints them.

### Development URLs

| What | URL |
| --- | --- |
| OpenAPI (Scalar) — `APP_ENV=development` only | http://localhost:4200/api/docs |
| Swagger UI — development only | http://localhost:4200/api/swagger |
| Spec JSON — development only | http://localhost:4200/api/openapi.json |
| Liveness / readiness | `/api/health` · `/api/readyz` (Postgres, Valkey, bucket) |
| Prometheus metrics | `/metrics` (bearer `METRICS_TOKEN`; open in dev when unset) |

## Make targets

All targets expect the dev shell (`devenv shell` or `nix develop`).

| Target | What it does |
| --- | --- |
| `make generate` | `templ generate` + `sqlc generate` + Tailwind build. Outputs are **committed** (`*_templ.go`, `internal/db/sqlc/`, `web/static/css/app.css`) |
| `make dev` | Live reload with [air](https://github.com/air-verse/air) (regenerates templ/CSS, rebuilds, restarts, and the open tab reloads itself) |
| `make build` | Static binaries in `./bin` (`server`, `seed`, `create-admin`) |
| `make test` / `make test-race` | `go test ./...` (with `-race`) |
| `make lint` | `golangci-lint run` + `sqlc vet` + `templ fmt -fail` |
| `make lint-infra` | hadolint (Dockerfile), shellcheck (`scripts/`), actionlint (workflows) |
| `make vuln` | `govulncheck ./...` |
| `make fmt` | gofumpt + goimports (`golangci-lint fmt`) + `templ fmt` |
| `make drift` | Regenerate and fail if generated code changed |
| `make migrate` / `make migrate-status` | goose `up` / `status` against `$DATABASE_URL` |
| `make seed` | `go run ./cmd/seed` |
| `make create-admin ARGS="--email you@example.com"` | Create or promote an admin (password on stdin) |
| `make backup` | `scripts/backup.sh manual` → `./backups/*.dump` |
| `make docker` | Build `lindaflor:dev` (runtime) and `lindaflor-migrate:dev` |
| `make docker-smoke` | Build both images and boot them against throwaway Postgres + Valkey |
| `make e2e` | Playwright suite against a throwaway stack (Postgres, Valkey, S3, Mailpit, fake Mercado Pago, TLS proxy) — see [`e2e/README.md`](e2e/README.md). Not part of `go test` |

devenv also exposes `generate`, `dev`, `lint`, `fmt`, `test`, `migrate`,
`seed`, `db-reset` (wipes the local Postgres state) and `ports` as shell
commands. Git hooks (installed by devenv) run gofumpt/goimports,
golangci-lint, `templ fmt`, nixfmt and whitespace checks.

## Seeding

`make seed` (or `go run ./cmd/seed`) inserts the admin user, store
settings, the default warehouse, the catalog from
`cmd/seed/data/catalog.json`, looks and conjuntos. It is idempotent by slug.

| Variable | Default | Description |
| --- | --- | --- |
| `SEED_ADMIN_EMAIL` | `admin@lindaflor.com` | Admin email |
| `SEED_ADMIN_PASSWORD` | `admin123` (dev only) | Required and must be strong (12+ chars) when `APP_ENV=production` |
| `SEED_ADMIN_NAME` | `Admin User` | Admin display name |
| `SEED_PRODUCT_COUNT` | `108` | Products (round-robin across catalog categories) |
| `SEED_SET_COUNT` | `12` | Conjuntos (top + calcinha bundles) |
| `SEED_PRICE_MIN` / `SEED_PRICE_MAX` | `3990` / `29990` | Price clamp (cents) |
| `SEED_STOCK_MIN` / `SEED_STOCK_MAX` | `5` / `30` | Stock range |
| `SEED_PIX_KEY`, `SEED_WHATSAPP_NUMBER`, `SEED_WHATSAPP_MESSAGE_TEMPLATE` | demo values | Store settings |

## Creating an admin

`create-admin` creates a user with a verified email and the `admin` role,
or promotes an existing user, and records an audit event. The password
comes from `ADMIN_PASSWORD_FILE` or one line on stdin — never argv — and
must pass the sign-up policy (length + zxcvbn).

```bash
printf '%s\n' "$ADMIN_PASSWORD" | go run ./cmd/create-admin --email dona@lindaflor.com.br --name "Dona"
go run ./cmd/create-admin --email cliente@example.com --no-password   # promote only
```

Re-running it with a password resets that user's password (admin
recovery). It refuses to run while migrations are pending.

## Configuration

Every variable can also be given Docker-secret style as `KEY_FILE` (a path
whose contents are the value); the plain variable wins, and empty values
count as unset. Outside production a `.env` file in the working directory
is loaded. Source of truth: `internal/config`.

### Core

| Variable | devenv value | Default / notes |
| --- | --- | --- |
| `APP_ENV` | `development` | **Unset means `production`** (fail closed). `development`/`dev` enable OpenAPI docs, dev cookies (no `Secure`/`__Host-`) and dev CSP; `test` is dev-like; anything else is treated like production |
| `HOST` | — | `127.0.0.1`. Containers must set `0.0.0.0` (the image does) |
| `PORT` | `4200` | `4200` |
| `APP_ORIGIN` | `http://localhost:4200` | Public origin (scheme + host). **Required in production**; used for the cross-origin check, absolute links in email, SEO and the MP webhook URL |
| `DATABASE_URL` | `postgres://postgres:postgres@127.0.0.1:4201/lindaflor?sslmode=disable` | **Required** |
| `PG_MAX_CONN` | — | 20 in production, 5 otherwise |
| `PG_ACQUIRE_TIMEOUT_SECS` / `PG_IDLE_TIMEOUT_SECS` | — | `5` / `600` |
| `MIGRATE_ON_BOOT` | — | On outside production, off in production (the migrate job owns the schema; the server refuses to boot with pending migrations) |
| `VALKEY_URL` | `redis://127.0.0.1:4202` | Same default; falls back to in-process state when unreachable |
| `S3_ENDPOINT`, `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY` | RustFS on 4203, `rustfsadmin` | All three required to enable storage (otherwise uploads are disabled and readyz reports `storage: skipped`) |
| `S3_REGION` / `S3_BUCKET` | `us-east-1` / `lindaflor` | Same defaults; path-style addressing |
| `TRUSTED_PROXY_HOPS` | — | `0` = directly exposed (proxy headers ignored). Set to the number of reverse proxies in front of the app |
| `TRUSTED_PROXY_NETWORKS` | — | Comma-separated CIDRs/IPs of trusted proxies |

### Secrets (required in production)

| Variable | Notes |
| --- | --- |
| `TOKEN_PEPPER` | HMAC pepper for one-time tokens and signed newsletter links (`openssl rand -hex 32`). The devenv value `dev-pepper-change-me` is refused in production |
| `TOTP_ENCRYPTION_KEY` | XChaCha20-Poly1305 key for TOTP secrets at rest: base64 of 32 bytes or 64 hex chars (`openssl rand -base64 32`). Dev/test generate an ephemeral key with a warning. **Losing it breaks every enrolled TOTP** |
| `METRICS_TOKEN` | Bearer token for `GET /metrics` |

### Mail

| Variable | Default / notes |
| --- | --- |
| `MAIL_TRANSPORT` | `log` (summary to the log) or `smtp`. Production refuses `log` unless `MAIL_ALLOW_LOG_IN_PRODUCTION=1` |
| `MAIL_FROM` | `From:` header, e.g. `Lindaflor <contato@lindaflor.com.br>`. Required in production |
| `SMTP_HOST` | Required with `smtp` |
| `SMTP_TLS` | `starttls` (default), `tls` (implicit) or `none` (dev only, e.g. mailpit) |
| `SMTP_PORT` | 587 starttls, 465 tls, 1025 none |
| `SMTP_USERNAME` / `SMTP_PASSWORD` | Optional credentials (never logged) |
| `MAIL_POLL_SECS` / `MAIL_MAX_ATTEMPTS` | `5` / `8` — outbox worker interval and attempts (exponential backoff) |

Email is queued in `email_outbox` in the same transaction as the change
that triggers it; a background worker delivers it with retries. To see real
messages locally run [mailpit](https://mailpit.axllent.org/)
(`docker run --rm -p 1025:1025 -p 8025:8025 axllent/mailpit`) with
`MAIL_TRANSPORT=smtp SMTP_HOST=127.0.0.1 SMTP_TLS=none`.

### Payments

| Variable | Default / notes |
| --- | --- |
| `PAYMENT_PROVIDER` | `static_pix` or `mercadopago`. Unset: `mercadopago` when `MP_ACCESS_TOKEN` is set, else `static_pix`. Production refuses `static_pix` unless `ALLOW_STATIC_PIX=1` |
| `MP_ACCESS_TOKEN` / `MP_WEBHOOK_SECRET` | Required for Mercado Pago. Webhook URL: `<APP_ORIGIN>/api/webhooks/mercadopago` (HMAC `x-signature`) |
| `MP_API_BASE_URL` | `https://api.mercadopago.com` (must be https in production) |
| `ALLOW_STATIC_PIX` | `1` to accept static PIX (manual confirmation) in production |

### Auth & abuse protection

| Variable | Default / notes |
| --- | --- |
| `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` | Google sign-in (PKCE). Unset disables it |
| `TURNSTILE_SITE_KEY` / `TURNSTILE_SECRET_KEY` (or `_FILE`) | Cloudflare Turnstile on sign-up, password reset (page + `/api/auth/*`), checkout (`intent=pay`), newsletter and avise-me. Both set: the widget renders, the CSP allows `https://challenges.cloudflare.com` and tokens are checked with siteverify (5s timeout). Unset: no-op. Sign-in has no CAPTCHA (lockouts) |
| `TURNSTILE_ENFORCE` | `1`: missing, invalid or unverifiable tokens are rejected (also when Cloudflare is unreachable); production refuses to boot without both keys. Off: report-only (failures logged, requests pass) |
| `TURNSTILE_VERIFY_URL` | siteverify endpoint override for tests; honored only when `APP_ENV` is development/dev/test |
| `PASSWORD_MIN_LEN` / `PASSWORD_MAX_LEN` / `PASSWORD_MIN_ZXCVBN_SCORE` | `8` / `128` / `3` |
| `RATE_LIMIT_<SCOPE>_PER_MIN` | Per-scope override, e.g. `RATE_LIMIT_AUTH_SIGN_IN_PER_MIN` (scope `:`/`-` → `_`, uppercased) |

Turnstile locally: Cloudflare publishes test keys. Site key
`1x00000000000000000000AA` with secret `1x0000000000000000000000000000000AA`
always passes; secret `2x0000000000000000000000000000000AA` always fails. Set
them with `TURNSTILE_ENFORCE=1` to exercise the widget end to end (needs
network access to `challenges.cloudflare.com`). Forms post the widget's
`cf-turnstile-response` field; JSON bodies take `captchaToken` (or the
`x-captcha-response` header); failures are a pt-BR toast / 422, or
`400 {"code":"CAPTCHA_FAILED"}` on the JSON API. The Go tests use a fake
siteverify server (`testutil.NewFakeTurnstile`) and never hit the network.

### Jobs & logging

| Variable | Default / notes |
| --- | --- |
| `RESERVATION_SWEEP_SECS` | `60` — expired stock-reservation sweeper interval |
| `ABANDONED_CART_ENABLED` / `ABANDONED_CART_AFTER_MINUTES` | on / `180` |
| `LOG_SAMPLE_RATE` | `1.0` — share of successful requests logged (errors and slow requests always) |
| `LOG_SLOW_THRESHOLD_MS` | `1000` |
| `LOG_FILE_PATH` / `LOG_FILE_MAX_BYTES` / `LOG_FILE_MAX_FILES` | Optional rotated JSON-lines file sink (10 MiB × 5); stdout always |

The app is same-origin only: no CORS headers, and cross-origin mutations are
rejected (net/http `CrossOriginProtection`) except the server-to-server
`POST /api/webhooks/mercadopago` (HMAC-authenticated) and the signed
newsletter unsubscribe link.

## Testing

```bash
make test             # go test ./...
make test-race        # what CI runs
go test ./internal/web/storefront/...   # one package
```

Integration tests start **one Postgres per test binary** from the
`initdb`/`postgres` on PATH (the nix shell provides them) and clone a
migrated template database per test (~10 ms). Without those binaries,
[embedded-postgres](https://github.com/fergusstrange/embedded-postgres)
downloads a server; `LINDAFLOR_TEST_PG=local|embedded` forces a mode.
Valkey is [miniredis](https://github.com/alicebob/miniredis); HTTP tests use
`internal/testutil` (full router, cookie-jar client). No running devenv
services are needed.

## Architecture

```
cmd/
  server/          config → pgx pool (migrate in dev / verify in prod) → valkey → router → http.Server (30 s drain) + jobs
  seed/            demo data (embedded catalog.json)
  create-admin/    production admin bootstrap
db/
  migrations/      goose SQL (00001_init.sql = consolidated schema); embedded into the binary
  queries/         sqlc query files, one per area → internal/db/sqlc (generated)
internal/
  config/          every env var (+ _FILE), production validation
  db/              pool, goose migrate / RequireSchemaCurrent, WithTx; sqlc/ generated
  httpx/           middleware: request id, client IP, recover, security headers (CSP/HSTS), body limits, origin protection; toast flash
  auth/            argon2id passwords, sessions (sha256 at rest), one-time tokens, TOTP + backup codes, step-up, Google OAuth
  store/           domain: catalog, cart, inventory, pricing, coupons, shipping, checkout, order FSM, returns, reviews, reports, wishlist, stock alerts
  mail/ payments/ storage/ ratelimit/ seo/ metrics/ logging/ audit/ jobs/ valkey/
  web/             router.go + one package per area (storefront, checkout, account, admin, authpages, authapi, platform)
                   components/ (templ UI kit), layout/, render/, format/
  testutil/        Postgres per test binary, miniredis, test server + client
web/
  static/          embedded assets: css/app.css (generated), js/, vendor/ (htmx 4, Alpine) — content-hashed URLs, immutable caching
  styles/input.css Tailwind v4 source
```

- **Rendering**: handlers render templ pages; forms work without JavaScript
  (POST → 303 redirect → GET with a toast cookie). htmx enhances them with
  fragment endpoints and `<hx-partial>` updates (e.g. cart badge); Alpine
  holds client-only UI state (cart sheet, sidebar, dialogs, gallery).
- **Data access**: SQL lives in `db/queries/*.sql`; sqlc generates typed Go
  in `internal/db/sqlc`. Transactions via `db.WithTx`. The order state
  machine in `internal/store` is the only writer of order status.
- **Migrations**: goose files in `db/migrations`, embedded in the binary.
  Development migrates on boot; production runs the `migrate` job first and
  the server refuses to start with pending migrations. Forward-only.
- **Background jobs** (`internal/jobs`): mail outbox worker, reservation
  sweeper, stock alerts, abandoned carts, maintenance — started by
  `cmd/server` and drained on shutdown.

## Deploy

| File | Purpose |
| --- | --- |
| `Dockerfile` | Multi-stage. Target `runtime` (default): `gcr.io/distroless/static-debian12:nonroot`, uid 10001, static `server` + `create-admin` + `seed`, `HOST=0.0.0.0`, `PORT=4200`, `APP_ENV=production`, `HEALTHCHECK` = `server -healthcheck` (GET `/api/health`). Target `migrate`: Postgres 17 client + goose + awscli running `scripts/migrate.sh` |
| `docker-compose.prod.yml` | Single host: `web`, one-shot `migrate`, `postgres:17`, `valkey:8`, `rustfs`, plus `backup` (profile `ops`). Secrets via files + `*_FILE`; web is read-only, `cap_drop: ALL`, readiness on `/api/readyz` |
| `scripts/migrate.sh` | Pre-migrate backup, then `goose up`. Also `migrate.sh backup [label]` / `migrate.sh info` |
| `scripts/backup.sh` | `pg_dump --format=custom`, verified with `pg_restore --list`, local rotation, optional S3 upload |
| `scripts/docker-smoke.sh` | Boots both images against throwaway Postgres + Valkey (used by CI) |
| `.github/workflows/ci.yml` | lint (golangci-lint, sqlc vet, templ fmt, hadolint, shellcheck, actionlint), generated-code drift, `go test -race`, govulncheck, `nix build`, Docker build + compose config + boot smoke |

### Build

```bash
make docker      # lindaflor:dev + lindaflor-migrate:dev
# or
docker build -t lindaflor:web .
docker build -t lindaflor:migrate --target migrate .
```

The build needs only the Go toolchain image and network for Go modules:
generated code and the compiled CSS are committed, and static assets,
templates, the OpenAPI spec and migrations are embedded in the binary.

### Run with docker compose

1. Create the secrets (one value per file; readable by the container users):

   ```bash
   mkdir -m 700 -p secrets && cd secrets
   openssl rand -hex 24 > postgres_password
   printf 'postgres://lindaflor:%s@postgres:5432/lindaflor?sslmode=disable' "$(cat postgres_password)" > database_url
   openssl rand -hex 24 > valkey_password
   printf 'redis://:%s@valkey:6379' "$(cat valkey_password)" > valkey_url
   openssl rand -hex 12 > s3_access_key
   openssl rand -hex 24 > s3_secret_key
   openssl rand -hex 32 > token_pepper
   openssl rand -base64 32 > totp_encryption_key   # back this up separately
   openssl rand -hex 32 > metrics_token
   chmod 644 ./* && cd ..
   ```

2. Start (mail and payment settings come from the environment or an `.env`
   next to the compose file):

   ```bash
   export APP_ORIGIN=https://lindaflor.com.br MAIL_FROM='Lindaflor <nao-responda@lindaflor.com.br>' \
          SMTP_HOST=smtp.example.com SMTP_USERNAME=… SMTP_PASSWORD=… \
          MP_ACCESS_TOKEN=… MP_WEBHOOK_SECRET=…
   docker compose -f docker-compose.prod.yml up -d --build
   ```

   Compose waits for Postgres, runs `migrate` to completion, then starts
   `web` on `127.0.0.1:4200` (`LINDAFLOR_HTTP_PORT` overrides). Put a TLS
   reverse proxy in front (`TRUSTED_PROXY_HOPS=1` is the compose default).

3. First admin (CLIs ship in the runtime image):

   ```bash
   printf '%s\n' "$ADMIN_PASSWORD" | docker compose -f docker-compose.prod.yml \
     run --rm -T --entrypoint create-admin web --email dona@lindaflor.com.br --name "Dona"
   ```

   Demo data (staging only): `docker compose -f docker-compose.prod.yml run --rm --entrypoint seed -e SEED_ADMIN_PASSWORD=… web`.

Notes:

- **Fresh schema.** The Go release uses a consolidated goose schema; it does
  not upgrade a database created by the Rust/sqlx release. Deploy onto an
  empty `lindaflor` database (dump and recreate an old `pgdata` volume).
  The baseline is a single `00001_init.sql`; a local database migrated by
  an earlier Go build (versions 2–3) needs `db-reset` (devenv shell)
  (or a fresh database) once.
- **Migrations never run in the web process in production**; on other
  platforms run the migrate image as a release job / init container:
  `docker run --rm -e DATABASE_URL=… lindaflor:migrate`. Write migrations to
  be compatible with the previous release (expand → deploy → contract).
- **Health**: `/api/health` is liveness (image `HEALTHCHECK`); `/api/readyz`
  checks Postgres, Valkey and the bucket (compose healthcheck, load
  balancers). The runtime image has no shell or curl — probe with
  `/app/server -healthcheck [-healthcheck-path=/api/readyz]`. `SIGTERM`
  drains in-flight requests for up to 30 s.

### Backups and restore

- **Scheduled**: `docker compose -f docker-compose.prod.yml run --rm backup`
  from host cron (e.g. `15 3 * * *`). Dumps land in the `backups` volume as
  `lindaflor-<UTC timestamp>-<label>.dump`; older than `BACKUP_KEEP_DAYS`
  (14) are rotated out, always keeping the newest `BACKUP_KEEP_MIN` (7).
- **Pre-migrate**: every migrate run dumps first and aborts the rollout if
  the dump fails (`SKIP_PRE_MIGRATE_BACKUP=1` skips it — never routine).
- **Off-site**: `BACKUP_S3_URI=s3://bucket/prefix` (+ `BACKUP_S3_ENDPOINT`
  for non-AWS, `BACKUP_AWS_ACCESS_KEY_ID` / `BACKUP_AWS_SECRET_ACCESS_KEY`).
  Use another region/provider with versioning and object lock.
- **Outside compose**: `DATABASE_URL=… BACKUP_DIR=/var/backups/lindaflor scripts/backup.sh`
  (needs `pg_dump` ≥ 17).
- **Restore** into an empty database, then start web (it verifies the goose
  version table):

  ```bash
  createdb lindaflor_restore
  pg_restore --no-owner --dbname "postgres://…/lindaflor_restore" lindaflor-<ts>-<label>.dump
  ```

- **Object storage** (product images, avatars) is not in `pg_dump`: enable
  bucket versioning and replicate it. **Valkey** holds sessions and
  rate-limit counters only (AOF + RDB in compose); losing it signs users out
  but loses no orders.
- **Restore drill** quarterly: restore the latest dump into a scratch
  database, boot a web container against it, check `/api/readyz` and an
  order page. The time taken is the real RTO.

### RPO / RTO

| Setup | RPO | RTO |
| --- | --- | --- |
| Nightly `pg_dump` only (default) | up to 24 h | ~15–30 min (provision Postgres, `pg_restore`, start web) |
| Pre-migrate dump | 0 for a failed migration | same as above |
| Managed Postgres with PITR / WAL archiving (recommended before launch) | ~5 min | ~30 min |

Launch targets: **RPO ≤ 15 min, RTO ≤ 1 h** — nightly dumps alone do not
meet the RPO for a shop taking orders; use PITR (managed Postgres, or
`archive_mode=on` + pgBackRest/wal-g) and keep the dumps as a logical copy.
