# Lindaflor

Brazilian swimwear storefront (UI copy in pt-BR): a server-rendered **[Topcoat](https://github.com/tokio-rs/topcoat) 0.9** web app — file-routed pages under `src/app/`, an admin area, cart/checkout with PIX, and a small JSON API (`/api/*`, OpenAPI in development).

- **App**: Rust 2024 + Topcoat 0.9 (axum-based; Tailwind + Iconify assets bundled by `topcoat asset bundle`)
- **Data**: PostgreSQL via sqlx (compile-time checked queries, `.sqlx/` offline cache committed)
- **Sessions / rate limits**: Valkey (Redis-compatible) via redis-rs
- **Object storage**: S3 API (RustFS locally) for product images and avatars
- **Environment**: Nix + [devenv](https://devenv.sh) 2.3 or newer for toolchain, Postgres, Valkey and RustFS

## Tech Stack

- **[Rust](https://www.rust-lang.org/learn)** 1.98.1 (pinned in `rust-toolchain.toml`)
- **[Topcoat](https://github.com/tokio-rs/topcoat)** — Web framework (views, routing, assets, UI components)
- **[sqlx](https://github.com/launchbadge/sqlx)** — Async SQL toolkit (Postgres)
- **[Valkey](https://valkey.io/)** — In-memory data store (Redis-compatible)
- **[RustFS](https://rustfs.com/)** — S3-compatible object storage
- **[postgresql_embedded](https://crates.io/crates/postgresql_embedded)** — Ephemeral Postgres for tests

## Quick start

Requires [devenv](https://devenv.sh) 2.3 or newer. That release fast-shuts Postgres on cancel; earlier versions can leave the data directory stuck in `stopping`.

### 1. Allow direnv (first time only)

```bash
direnv allow
```

This auto-activates the devenv shell when entering the project directory — no need to manually run `devenv shell`.

### 2. Start the stack

```bash
devenv up
```

Starts PostgreSQL (port 4201), Valkey (port 4202), pgweb (port 4205), and the app under `topcoat dev` (build, asset bundle, watch, hot reload).

### 3. Run migrations

```bash
sqlx migrate run
```

### 4. Seed the database

```sh
cargo run --bin seed
```

Seeds the database with an admin user and sample commerce data. Optional environment variables override the defaults:

| Variable | Default | Description |
|---|---|---|
| `SEED_ADMIN_EMAIL` | `admin@lindaflor.com` | Admin user email |
| `SEED_ADMIN_PASSWORD` | `admin123` | Admin user password |
| `SEED_ADMIN_NAME` | `Admin User` | Admin user display name |
| `SEED_PRODUCT_COUNT` | `108` | Number of products to seed (round-robin across categories from `src/bin/seed/data/catalog.json`) |
| `SEED_SET_COUNT` | `12` | Number of conjuntos (top + calcinha bundles) to seed |
| `SEED_PRICE_MIN` | `3990` | Clamp fixture prices to this minimum (cents) |
| `SEED_PRICE_MAX` | `29990` | Clamp fixture prices to this maximum (cents) |
| `SEED_STOCK_MIN` | `5` | Minimum stock quantity |
| `SEED_STOCK_MAX` | `30` | Maximum stock quantity |

Re-running seed is idempotent by product/set slug: existing demo SKUs are skipped, new catalog slugs still insert. For a clean catalog, reset the database volume first.

### 5. Run the server

With the stack up (`devenv up`), the app is already watched by `topcoat dev`. Standalone:

```bash
topcoat dev --bin lindaflor
```

The app listens on `http://localhost:4200`.

OpenAPI docs (served only when `APP_ENV=development`):

| UI | URL |
| --- | --- |
| Scalar | http://localhost:4200/api/docs |
| Swagger | http://localhost:4200/api/swagger |
| Spec JSON | http://localhost:4200/api/openapi.json |

pgweb is a local Postgres viewer started by `devenv up` at http://127.0.0.1:4205. It does not manage schema — sqlx owns migrations.

## Configuration

devenv exports the process environment. `DATABASE_URL` follows the Postgres port it allocated.

| Variable                  | Set by devenv                                             | Description |
| ------------------------- | --------------------------------------------------------- | ----------- |
| `HOST`                    | _(unset → `127.0.0.1`)_                                   | Bind address. Defaults to loopback; **containers must set `0.0.0.0`** (the Docker image does) |
| `PORT`                    | `4200`                                                    | HTTP listen port |
| `DATABASE_URL`            | `postgres://postgres:postgres@127.0.0.1:$PGPORT/topcoat`  | PostgreSQL connection |
| `VALKEY_URL`              | `redis://127.0.0.1:4202`                                  | Valkey connection |
| `APP_ENV`                 | `development`                                             | `development` / `dev` enables OpenAPI docs; `production` (the default if unset) requires `APP_ORIGIN` |
| `APP_ORIGIN`              | `http://localhost:4200`                                   | Origin policy check (CSRF checks). |
| `S3_ENDPOINT`             | `http://127.0.0.1:4203`                                   | RustFS S3 API endpoint |
| `S3_REGION`               | `us-east-1`                                               | S3 region |
| `S3_ACCESS_KEY_ID`        | `rustfsadmin`                                             | RustFS access key |
| `S3_SECRET_ACCESS_KEY`    | `rustfsadmin`                                             | RustFS secret key |
| `S3_BUCKET`               | `lindaflor`                                               | Object bucket |
| `TOKEN_PEPPER`            | `dev-pepper-change-me`                                    | Pepper for one-time tokens |
| `LOG_SAMPLE_RATE`         | `1`                                                       | Sample rate for logging |
| `LOG_SLOW_THRESHOLD_MS`   | `1000`                                                    | Slow request threshold in milliseconds |
| `TRUSTED_PROXY_HOPS`      | `0`                                                       | Number of reverse proxies in front of the app (Topcoat trusted proxies); `0` = directly exposed, proxy headers ignored |
| `TRUSTED_PROXY_NETWORKS`  | _(unset)_                                                 | Comma-separated trusted proxy CIDRs/IPs (e.g. `10.0.0.0/8, 192.168.0.0/16`), combined with `TRUSTED_PROXY_HOPS` |

Avatars are stored in RustFS (S3 API). The console is http://127.0.0.1:4204.

The app is same-origin only: there is no CORS layer and no `Access-Control-*` headers. Cross-origin browser POSTs (and other mutations) are rejected with 403 by Topcoat `OriginPolicy`. There is no public cross-origin API.

## Development

```bash
# Format
cargo fmt

# Lint
cargo clippy --all-targets

# Run tests (uses embedded PostgreSQL)
cargo test

# Live server (build + assets + reload)
topcoat dev --bin lindaflor
```

## Reset database

```bash
devenv shell db-reset
devenv up   # recreates Postgres; server runs `sqlx migrate run` before build
```

After a reset the DB is empty. `sqlx::query!` needs tables at compile time, so migrations must run before the first build (`devenv up` does this automatically).

### sqlx offline cache (`.sqlx/`)

`.sqlx/` is committed: the Docker build and CI compile with `SQLX_OFFLINE=true` and never touch a database. After changing any `query!`/`query_as!` or adding a migration, regenerate and commit it:

```bash
devenv shell prepare-sqlx   # sqlx migrate run + cargo sqlx prepare -- --all-targets
```

CI fails (`cargo sqlx prepare --check`) when the cache is stale.

### Migration `0002` starts with `DROP TABLE … CASCADE`

`migrations/0002_commerce.sql` begins by dropping every commerce table (orders, products, inventory, …). sqlx never re-runs an applied migration, so this is harmless through `sqlx migrate run`, but **never run that file by hand** (`psql -f`) against a database with data — it wipes the catalog and all orders. Do not edit it either: changing an applied migration breaks its checksum and the app refuses to boot.

## Deploy & backups

Production artifacts:

| File | Purpose |
| --- | --- |
| `Dockerfile` | Multi-stage build. Default target `runtime`: slim Debian, non-root (uid 10001), CA certs, `HOST=0.0.0.0`, `APP_ENV=production`, `HEALTHCHECK` on `/api/health`, binary + Topcoat asset bundle in `/app`. Target `migrate`: `sqlx-cli` + `pg_dump` (Postgres 17) running `scripts/migrate.sh`. |
| `docker-compose.prod.yml` | Single-host stack: `web`, one-shot `migrate` job, `postgres`, `valkey`, `rustfs`, plus a `backup` service (profile `ops`). Secrets via files + `*_FILE`. |
| `scripts/migrate.sh` | Pre-migrate backup, then `sqlx migrate run`. Also `migrate.sh backup` / `migrate.sh info`. |
| `scripts/backup.sh` | `pg_dump --format=custom`, verified with `pg_restore --list`, local rotation, optional S3 upload. |
| `.github/workflows/ci.yml` | fmt, clippy `-D warnings`, tests (embedded Postgres), `cargo audit`, `sqlx migrate run` + `cargo sqlx prepare --check`, hadolint, Docker build + boot smoke. |

### Build

```bash
docker build -t lindaflor:web .                       # runtime (default target)
docker build -t lindaflor:migrate --target migrate .
```

The build needs no database (`SQLX_OFFLINE=true` + committed `.sqlx/`) but does need outbound network: Topcoat's build script downloads the Tailwind standalone CLI and Iconify icon sets. Dependencies are cached in their own layer (cargo-chef), so source-only changes rebuild just the crate. `topcoat asset bundle --release` writes the CSS/JS/icon bundle next to the binary (`/app/assets`), where `AssetBundle::load()` expects it — ship the binary and its `assets/` together, from the same build.

### Run (docker compose)

1. Create the secrets (one value per file, no surrounding whitespace; files are bind-mounted, so make them readable by the container users):

   ```bash
   mkdir -m 700 -p secrets && cd secrets
   openssl rand -hex 24 > postgres_password
   printf 'postgres://lindaflor:%s@postgres:5432/lindaflor' "$(cat postgres_password)" > database_url
   openssl rand -hex 24 > valkey_password
   printf 'redis://:%s@valkey:6379' "$(cat valkey_password)" > valkey_url
   openssl rand -hex 12 > s3_access_key
   openssl rand -hex 24 > s3_secret_key
   openssl rand -hex 32 > token_pepper
   openssl rand -base64 32 > totp_encryption_key   # back this up separately: losing it breaks every TOTP
   openssl rand -hex 32 > metrics_token
   chmod 644 ./* && cd ..
   ```

2. Start: `APP_ORIGIN=https://<your-domain> docker compose -f docker-compose.prod.yml up -d --build`. Compose waits for Postgres, runs the `migrate` job to completion, then starts `web`. Put a TLS reverse proxy in front of `127.0.0.1:4200` (`TRUSTED_PROXY_HOPS=1` is the compose default).
3. First admin: there is no production admin-bootstrap command yet. The `seed` binary is not shipped in the image and also inserts demo catalog data, so do not point it at production; until a one-shot admin-create exists, sign up normally and promote that user (`UPDATE users SET role = 'admin' WHERE email = …`) in a `psql` session.

Notes:

- **`HOST`**: the binary binds `127.0.0.1` unless `HOST` is set. The image sets `HOST=0.0.0.0`; set it yourself if you run the binary in any other container/VM.
- **Migrations never run in the web process in production.** `MIGRATE_ON_BOOT` defaults off when `APP_ENV=production`, and the server refuses to boot while migrations are pending, dirty, or checksum-mismatched. Always run the migrate job first (compose does this; on other platforms use it as a release/pre-deploy job or init container: `docker run --rm -e DATABASE_URL=… lindaflor:migrate`).
- Migrations are forward-only (no down files). Write them to be compatible with the previous release still running (expand → deploy → contract), since old replicas keep serving during a rollout.
- Health: `/api/health` is liveness (image `HEALTHCHECK`); `/api/readyz` checks Postgres, Valkey and the bucket (use it for load-balancer readiness). `SIGTERM` drains in-flight requests for up to 30s.
- `GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` are read from plain env only (no `*_FILE` variant yet).

### Backups and restore

- **Scheduled**: run `docker compose -f docker-compose.prod.yml run --rm backup` from host cron (e.g. daily `15 3 * * *`). Dumps land in the `backups` volume as `lindaflor-<UTC timestamp>-<label>.dump`; dumps older than `BACKUP_KEEP_DAYS` (14) are removed, always keeping the newest `BACKUP_KEEP_MIN` (7).
- **Pre-migrate**: every migrate job writes a `…-pre-migrate.dump` first and aborts the rollout if the dump fails (`SKIP_PRE_MIGRATE_BACKUP=1` only for the very first deploy, if at all).
- **Off-site**: set `BACKUP_S3_URI=s3://bucket/prefix` (+ `BACKUP_S3_ENDPOINT` for non-AWS, `BACKUP_AWS_ACCESS_KEY_ID` / `BACKUP_AWS_SECRET_ACCESS_KEY`). Use a bucket in another region/provider with versioning and object lock/lifecycle; a backup on the same host as the database is not a backup.
- **Outside compose**: `DATABASE_URL=… BACKUP_DIR=/var/backups/lindaflor scripts/backup.sh` (needs `pg_dump` ≥ the server's major version, 17).
- **Restore** into an empty database, then start the web process (it verifies the migration table matches):

  ```bash
  createdb lindaflor_restore
  pg_restore --no-owner --dbname "postgres://…/lindaflor_restore" lindaflor-<ts>-<label>.dump
  ```

- **Object storage** (product images, avatars): enable bucket versioning and replicate to a second location; `pg_dump` does not include it. **Valkey** holds sessions and rate-limit counters only (AOF + RDB in compose); losing it signs users out but loses no orders, so it is not backed up off-host.
- **Restore drill**: quarterly, restore the latest dump into a scratch database, boot a web container against it, and check `/api/readyz` and an order page. Record the time taken — that is the real RTO.

### RPO / RTO

| Setup | RPO (max data loss) | RTO (time to restore) |
| --- | --- | --- |
| Nightly `pg_dump` only (this repo, default) | up to 24 h (plus any orders since the last dump) | ~15–30 min for the current data size: provision Postgres, `pg_restore`, start web |
| Pre-migrate dump | 0 for a failed migration (taken seconds before it) | same as above |
| Managed Postgres with PITR / WAL archiving (recommended before launch) | ~5 min (WAL segment/archive interval) | ~30 min (point-in-time restore + repoint `DATABASE_URL`) |

Targets for launch: **RPO ≤ 15 min, RTO ≤ 1 h**. Nightly dumps alone do not meet the RPO for a shop taking orders — use a managed Postgres with PITR (7-day minimum retention) or add WAL archiving (`archive_mode=on` + `pgBackRest`/`wal-g` to off-site storage) to the self-hosted `postgres` service, and keep the nightly custom-format dumps as a second, logical copy.

## Project structure

```
├── migrations/           # SQL migrations (sqlx)
├── src/
│   ├── main.rs           # Entry point
│   ├── config.rs         # Environment config
│   ├── db.rs             # PostgreSQL pool + migrations
│   ├── openapi.rs        # OpenAPI spec + Scalar/Swagger routes
│   ├── valkey.rs         # Valkey client
│   ├── storage.rs        # S3-compatible object store (RustFS)
│   ├── bin/
│   │   └── seed/
│   │       ├── main.rs       # Seed binary entry
│   │       └── seeders/      # Database seeders
│   │           ├── mod.rs    # Seeder orchestration
│   │           ├── admin.rs  # Admin user seeder
│   │           └── commerce.rs # Commerce data seeder
│   └── app/              # Pages
├── tests/
│   └── health.rs         # Integration test (embedded postgres)
├── devenv.nix            # devenv configuration
├── devenv.yaml           # devenv inputs
├── rust-toolchain.toml   # Pinned Rust 1.98.1
└── .envrc                # direnv auto-activation
```
