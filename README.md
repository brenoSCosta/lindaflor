# topcoat

Rust web API built with axum, sqlx (PostgreSQL), and Valkey.

- **Backend**: Rust with Axum, sqlx, and redis-rs (Valkey-compatible)
- **Environment**: Nix + [devenv](https://devenv.sh) 2.3 or newer for toolchain, Postgres, and Valkey services
- **Auth storage**: Valkey for sessions and secondary auth data

## Tech Stack

- **[Rust](https://www.rust-lang.org/learn)** — Systems language
- **[Axum](https://docs.rs/axum/latest/axum/)** — Web framework
- **[sqlx](https://github.com/launchbadge/sqlx)** — Async SQL toolkit (Postgres)
- **[Valkey](https://valkey.io/)** — In-memory data store (Redis-compatible)
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
| `SEED_PRODUCT_COUNT` | `12` | Number of products to seed |
| `SEED_COLLECTIONS` | `Verão 2026,Clássicos,Pôr do sol` | Comma-separated collection names |
| `SEED_PRICE_MIN` | `6990` | Minimum product price in cents |
| `SEED_PRICE_MAX` | `29990` | Maximum product price in cents |
| `SEED_STOCK_MIN` | `5` | Minimum stock quantity |
| `SEED_STOCK_MAX` | `30` | Maximum stock quantity |

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
