FROM oven/bun:latest AS base

# Stage 1: Install all dependencies (prod + dev) needed for building
FROM base AS deps

WORKDIR /app

COPY package.json bun.lock turbo.json bunfig.toml ./
COPY apps/server/package.json ./apps/server/
COPY apps/web/package.json ./apps/web/
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
RUN bun install

# Stage 2: Build the application
FROM base AS build

WORKDIR /app

# Bring the FULL installed dependency tree from the deps stage. bun uses an
# isolated linker (bunfig.toml), so deps live in per-workspace node_modules
# (apps/web/node_modules, packages/*/node_modules) with relative symlinks, not
# just the root. Copying only root node_modules left the build unable to resolve
# vite / @sveltejs/* / @lindaflor/* on Railway (where node_modules is gitignored
# and absent from the build context).
COPY --from=deps /app ./

# Overlay the actual source on top (merges; the copied node_modules are kept).
COPY apps/web ./apps/web
COPY packages ./packages

ARG VITE_SERVER_URL=http://localhost:5174
ARG VITE_WEB_PORT=5175
ENV VITE_SERVER_URL=$VITE_SERVER_URL
ENV VITE_WEB_PORT=$VITE_WEB_PORT
ENV VITE_NODE_ENV=production

# Build the web app (adapter-static -> apps/web/build, fully prerendered)
WORKDIR /app/apps/web
RUN bun run build

# Stage 3: Production runtime - only static files, no node_modules
# Use caddy alpine (tiny image, built-in zstd/gzip + SPA fallback)
FROM caddy:alpine AS release

# Install curl for the compose healthcheck (not bundled in caddy:alpine)
RUN apk add --no-cache curl && \
    rm -rf /var/cache/apk/*

# Caddy config (adapter-static output served from /srv, SPA fallback, asset caching)
COPY apps/web/Caddyfile /etc/caddy/Caddyfile

# Copy built static files to Caddy's web root
COPY --from=build /app/apps/web/dist /srv

EXPOSE 5175

# Run Caddy with the project Caddyfile
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]