# syntax=docker/dockerfile:1.7
#
# Lindaflor production image.
#
# Targets:
#   runtime (default) — the web server + its Topcoat asset bundle. Never
#                       migrates (`MIGRATE_ON_BOOT` is off in production);
#                       boot aborts if migrations are pending.
#   migrate           — one-shot job: pre-migrate `pg_dump` backup, then
#                       `sqlx migrate run` (`scripts/migrate.sh`). Also runs
#                       `scripts/backup.sh` for scheduled backups.
#
#   docker build -t lindaflor:web .
#   docker build -t lindaflor:migrate --target migrate .
#
# sqlx query macros compile against the committed `.sqlx/` cache
# (`SQLX_OFFLINE=true`), so no database is needed at build time. Regenerate the
# cache with `prepare-sqlx` after changing any `query!` or migration.
#
# Building needs outbound network: Topcoat's build script downloads the
# Tailwind standalone CLI (GitHub) and Iconify icon sets (jsDelivr).

ARG RUST_VERSION=1.98.1
ARG DEBIAN_RELEASE=bookworm
ARG PG_MAJOR=17
ARG CARGO_CHEF_VERSION=0.1.73
ARG TOPCOAT_CLI_VERSION=0.9.0
ARG SQLX_CLI_VERSION=0.8.6

# ---------------------------------------------------------------------------
# Toolchain: pinned Rust (matches rust-toolchain.toml) + build tools.
# ---------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS toolchain
ARG RUST_VERSION
ARG CARGO_CHEF_VERSION
ARG TOPCOAT_CLI_VERSION
# rust-toolchain.toml pins the same channel; keep rustup from fetching the
# dev-only components (rust-analyzer, rust-src) inside the image.
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION} \
    CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0
RUN cargo install --locked cargo-chef --version "${CARGO_CHEF_VERSION}" \
 && cargo install --locked topcoat-cli --version "${TOPCOAT_CLI_VERSION}"
WORKDIR /src

# ---------------------------------------------------------------------------
# Recipe: dependency fingerprint (changes only when manifests/lockfile do).
# ---------------------------------------------------------------------------
FROM toolchain AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---------------------------------------------------------------------------
# Builder: cook deps (cached layer), then build the app + asset bundle.
# ---------------------------------------------------------------------------
FROM toolchain AS builder
ENV SQLX_OFFLINE=true
COPY --from=planner /src/recipe.json recipe.json
# cargo-chef only skeletons workspace members; the non-member path dev-dep
# must exist for Cargo to resolve the lockfile (it is not compiled here).
COPY crates/test-support crates/test-support
RUN cargo chef cook --release --locked --recipe-path recipe.json --bin lindaflor
COPY . .
# `topcoat asset bundle` runs `cargo build` for the binary, then writes the
# asset bundle (CSS, JS, icons, fonts + manifest.toml) to
# `target/release/assets`, which the server loads from next to its executable.
# `create-admin` (admin bootstrap CLI, see README) ships in the runtime image.
RUN test -d .sqlx || { echo ".sqlx/ query cache missing: run prepare-sqlx and commit it" >&2; exit 1; } \
 && topcoat asset bundle --release --bin lindaflor \
 && test -f target/release/assets/manifest.toml \
 && cargo build --release --locked --bin create-admin \
 && mkdir -p /out \
 && cp target/release/lindaflor /out/lindaflor \
 && cp target/release/create-admin /out/create-admin \
 && cp -r target/release/assets /out/assets

# ---------------------------------------------------------------------------
# sqlx-cli for the migrate job (same sqlx line as Cargo.toml: 0.8).
# ---------------------------------------------------------------------------
FROM rust:${RUST_VERSION}-slim-${DEBIAN_RELEASE} AS sqlx-cli
ARG SQLX_CLI_VERSION
RUN cargo install --locked sqlx-cli --version "${SQLX_CLI_VERSION}" \
      --no-default-features --features rustls,postgres

# ---------------------------------------------------------------------------
# migrate: one-shot job image (pg_dump matches the server major version).
# ---------------------------------------------------------------------------
FROM postgres:${PG_MAJOR}-${DEBIAN_RELEASE} AS migrate
# awscli: optional off-site upload in scripts/backup.sh (BACKUP_S3_URI).
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates awscli \
 && rm -rf /var/lib/apt/lists/*
COPY --from=sqlx-cli /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
WORKDIR /app
COPY migrations ./migrations
COPY scripts/backup.sh scripts/migrate.sh ./scripts/
RUN mkdir -p /backups && chown postgres:postgres /backups
ENV BACKUP_DIR=/backups
USER postgres
VOLUME ["/backups"]
ENTRYPOINT ["/app/scripts/migrate.sh"]

# ---------------------------------------------------------------------------
# runtime: slim, non-root.
# ---------------------------------------------------------------------------
FROM debian:${DEBIAN_RELEASE}-slim AS runtime
# ca-certificates: rustls-platform-verifier reads the system trust store for
# outbound HTTPS (OAuth, S3, mail). curl: container HEALTHCHECK only.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl tini \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 lindaflor \
 && useradd --system --uid 10001 --gid lindaflor --home-dir /app --shell /usr/sbin/nologin lindaflor
WORKDIR /app
# The bundle must sit beside the executable (`AssetBundle::load`).
COPY --from=builder --chown=root:root /out/lindaflor /app/lindaflor
COPY --from=builder --chown=root:root /out/create-admin /usr/local/bin/create-admin
COPY --from=builder --chown=root:root /out/assets /app/assets
# HOST defaults to 127.0.0.1 in the binary; containers must listen on all
# interfaces. APP_ENV=production also disables MIGRATE_ON_BOOT and dotenv.
ENV HOST=0.0.0.0 \
    PORT=4200 \
    APP_ENV=production \
    RUST_LOG=info
USER lindaflor:lindaflor
EXPOSE 4200
# Liveness only (cheap). Orchestrators should use /api/readyz for readiness.
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=3 \
  CMD ["sh", "-c", "curl -fsS \"http://127.0.0.1:${PORT}/api/health\" >/dev/null || exit 1"]
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/app/lindaflor"]
