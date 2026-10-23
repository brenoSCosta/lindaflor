# syntax=docker/dockerfile:1.7
#
# Lindaflor production image (Go).
#
# Targets:
#   runtime (default) — distroless/static, non-root (uid 10001): the web
#                       server plus the `create-admin` and `seed` CLIs.
#                       Never migrates (`MIGRATE_ON_BOOT` is off in
#                       production); boot aborts if migrations are pending.
#   migrate           — one-shot job: pre-migrate `pg_dump` backup, then
#                       `goose up` (`scripts/migrate.sh`). Also runs
#                       `scripts/backup.sh` for scheduled backups.
#
#   docker build -t lindaflor:web .
#   docker build -t lindaflor:migrate --target migrate .
#
# Generated code is committed (templ `*_templ.go`, sqlc `internal/db/sqlc`,
# Tailwind `web/static/css/app.css`), so the build needs only the Go
# toolchain; CI fails when those outputs drift (`make drift`). Static assets,
# templates, the OpenAPI spec and the goose migrations are embedded in the
# server binary.

ARG GO_VERSION=1.27.3
ARG DEBIAN_RELEASE=bookworm
ARG PG_MAJOR=17
ARG GOOSE_VERSION=v3.29.0

# ---------------------------------------------------------------------------
# Builder: cross-compiles on the build platform (CGO off → fully static).
# ---------------------------------------------------------------------------
FROM --platform=$BUILDPLATFORM golang:${GO_VERSION}-${DEBIAN_RELEASE} AS builder
ARG TARGETOS
ARG TARGETARCH
ARG GOOSE_VERSION
ENV CGO_ENABLED=0 \
    GOOS=${TARGETOS} \
    GOARCH=${TARGETARCH} \
    GOTOOLCHAIN=local \
    GOFLAGS=-trimpath
WORKDIR /src

# Dependency layer: changes only with go.mod/go.sum.
COPY go.mod go.sum ./
RUN --mount=type=cache,target=/go/pkg/mod \
    go mod download -x

# goose CLI for the migrate image: same version as go.mod, Postgres only.
# (DL3062 cannot see that the version comes from ${GOOSE_VERSION}.)
# hadolint ignore=DL3062
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    GOBIN=/out go install -ldflags "-s -w" \
      -tags "no_clickhouse no_libsql no_mssql no_mysql no_sqlite3 no_vertica no_ydb" \
      "github.com/pressly/goose/v3/cmd/goose@${GOOSE_VERSION}" \
 && if [ -d "/out/${TARGETOS}_${TARGETARCH}" ]; then mv "/out/${TARGETOS}_${TARGETARCH}/goose" /out/goose; fi \
 && test -f /out/goose

COPY cmd ./cmd
COPY internal ./internal
COPY db ./db
COPY web ./web
RUN --mount=type=cache,target=/go/pkg/mod \
    --mount=type=cache,target=/root/.cache/go-build \
    test -s web/static/css/app.css || { echo "web/static/css/app.css missing: run make generate and commit it" >&2; exit 1; } \
 && go build -ldflags "-s -w" -o /out/ ./cmd/server ./cmd/create-admin ./cmd/seed

# ---------------------------------------------------------------------------
# migrate: one-shot job image (pg_dump matches the server major version).
# ---------------------------------------------------------------------------
FROM postgres:${PG_MAJOR}-${DEBIAN_RELEASE} AS migrate
# awscli: optional off-site upload in scripts/backup.sh (BACKUP_S3_URI).
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates awscli \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /out/goose /usr/local/bin/goose
WORKDIR /app
COPY db/migrations /migrations
COPY scripts/backup.sh scripts/migrate.sh ./scripts/
RUN mkdir -p /backups && chown postgres:postgres /backups
ENV BACKUP_DIR=/backups \
    MIGRATIONS_DIR=/migrations
USER postgres
VOLUME ["/backups"]
ENTRYPOINT ["/app/scripts/migrate.sh"]

# ---------------------------------------------------------------------------
# runtime: distroless static (CA certs + tzdata, no shell), non-root.
# ---------------------------------------------------------------------------
FROM gcr.io/distroless/static-debian12:nonroot AS runtime
COPY --from=builder /out/server /app/server
COPY --from=builder /out/create-admin /usr/local/bin/create-admin
COPY --from=builder /out/seed /usr/local/bin/seed
WORKDIR /app
# HOST defaults to 127.0.0.1 in the binary; containers must listen on all
# interfaces. APP_ENV=production also disables MIGRATE_ON_BOOT and .env.
ENV HOST=0.0.0.0 \
    PORT=4200 \
    APP_ENV=production
USER 10001:10001
EXPOSE 4200
# Liveness only (cheap). Orchestrators should use /api/readyz for readiness.
# No curl in distroless: the server binary probes itself.
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=3 \
  CMD ["/app/server", "-healthcheck"]
# Go installs its own SIGTERM handler (30s graceful drain) and spawns no
# children, so no init process is needed as PID 1.
ENTRYPOINT ["/app/server"]
