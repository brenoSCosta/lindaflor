FROM oven/bun:latest AS base

# Stage 1: Install all dependencies (prod + dev) needed for building
FROM base AS deps

WORKDIR /app

COPY package.json bun.lock turbo.json bunfig.toml ./
COPY apps/server/package.json ./apps/server/
COPY apps/web/package.json ./apps/web/
COPY apps/native/package.json ./apps/native/
COPY packages/api/package.json ./packages/api/
COPY packages/auth/package.json ./packages/auth/
COPY packages/config/package.json ./packages/config/
COPY packages/db/package.json ./packages/db/
COPY packages/e2e/package.json ./packages/e2e/
COPY packages/env/package.json ./packages/env/
COPY packages/mail/package.json ./packages/mail/
COPY packages/s3/package.json ./packages/s3/
COPY packages/valkey/package.json ./packages/valkey/

# Install ALL dependencies (including devDependencies needed for build)
RUN bun install --frozen-lockfile

# Stage 2: Build the application
FROM base AS build

WORKDIR /app

# Bring the FULL installed dependency tree from the deps stage. bun uses an
# isolated linker (bunfig.toml), so deps live in per-workspace node_modules
# (apps/server/node_modules, packages/*/node_modules) with relative symlinks,
# not just the root. Copying only root node_modules left the build unable to
# resolve elysia / @orpc/* / @lindaflor/* on Railway (where node_modules is
# gitignored and absent from the build context).
COPY --from=deps /app ./

# Overlay the actual source on top (merges; the copied node_modules are kept).
COPY apps/server ./apps/server
COPY packages ./packages

# Compile the server into a standalone binary with maximum optimization.
# `bun run compile` bundles every dependency, so the runtime needs no node_modules.
# Kept as its own RUN so a compile failure fails the build here (with its real
# error) instead of being swallowed and surfacing later as a missing-file COPY.
WORKDIR /app/apps/server
RUN bun run build
# Strip debug symbols to reduce binary size (optional — binutils may be absent).
# Note: UPX compression removed - it breaks Bun binary compatibility.
RUN strip --strip-all dist/server 2>/dev/null || true

# Stage 3: Production runtime - only the compiled binary, no node_modules
# Use distroless cc variant (includes C++ libraries needed for Bun)
# Bun compiled binaries require libstdc++ which is in the cc variant
# Distroless provides glibc (needed for Bun binaries) and C++ libs without shell or package manager
FROM gcr.io/distroless/cc-debian12:nonroot AS runner

WORKDIR /app

# Copy the compiled server binary (produced by `bun run build` -> ./server)
COPY --from=build /app/apps/server/dist/server ./server

# Copy migrations folder - required for database migrations at runtime
COPY --from=build /app/packages/db/src/migrations ./migrations

# Distroless images run as non-root by default (user: nonroot, uid: 65532)
# The binary is already executable, no chmod needed

ENV NODE_ENV=production

EXPOSE 9000

# Run the compiled binary (no Bun runtime needed!)
CMD ["./server"]