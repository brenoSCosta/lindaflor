{
  pkgs,
  lib,
  ...
}:

{
  packages = [
    pkgs.clang
    pkgs.mold
    pkgs.openssl
    pkgs.pgweb
    pkgs.pkg-config
    pkgs.sqlx-cli
    pkgs.valkey
    # Runtime libs for postgresql_embedded (theseus) binaries.
    pkgs.krb5
    pkgs.lz4
    pkgs.zstd
    pkgs.libxml2
    pkgs.zlib
  ];

  env.PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  # postgresql_embedded downloads glibc-linked PG binaries that need these at runtime.
  env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
    pkgs.krb5
    pkgs.lz4
    pkgs.zstd
    pkgs.libxml2
    pkgs.openssl
    pkgs.zlib
  ];
  env.DATABASE_URL = "postgres://postgres:postgres@127.0.0.1:4201/topcoat";
  # Ad-hoc cargo and rust-analyzer read .sqlx/ instead of opening a connection per query.
  # `devenv up` overrides this and rewrites the cache. See scripts.cargo-with-sqlx.
  env.SQLX_OFFLINE = "true";
  env.PGPORT = lib.mkForce "4201";
  env.VALKEY_URL = "redis://127.0.0.1:4202";
  env.S3_ENDPOINT = "http://127.0.0.1:4203";
  env.S3_REGION = "us-east-1";
  env.S3_ACCESS_KEY_ID = "rustfsadmin";
  env.S3_SECRET_ACCESS_KEY = "rustfsadmin";
  env.S3_BUCKET = "lindaflor";
  env.PORT = "4200";
  env.APP_ENV = "development";
  env.GOOGLE_CLIENT_ID = "fake-client.apps.googleusercontent.com";
  env.GOOGLE_CLIENT_SECRET = "GOCSPX-fake";

  languages.rust = {
    enable = true;
    channel = "stable";
    version = "1.98.1";
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
      "rust-src"
    ];
  };

  git-hooks.hooks = {
    check-yaml.enable = true;
    check-merge-conflicts.enable = true;
    end-of-file-fixer.enable = true;
    trim-trailing-whitespace.enable = true;
    nixfmt.enable = true;

    rustfmt = {
      enable = true;
      name = "cargo fmt";
      entry = "cargo fmt --check";
      files = "\\.rs$";
      pass_filenames = false;
    };
    cargo-clippy = {
      enable = true;
      name = "cargo clippy";
      entry = "cargo-with-sqlx cargo clippy --all-targets";
      files = "\\.rs$";
      pass_filenames = false;
    };
  };

  services.postgres = {
    enable = true;
    package = pkgs.postgresql_17;
    listen_addresses = "127.0.0.1";
    port = 4201;
    initialDatabases = [
      {
        name = "topcoat";
        user = "postgres";
        pass = "postgres";
      }
    ];
    hbaConf = ''
      local   all             all                                     trust
      host    all             all             127.0.0.1/32            trust
      host    all             all             ::1/128                 trust
    '';
    settings.port = lib.mkForce 4201;
  };

  processes.postgres.exec = lib.mkForce ''
    set -euo pipefail
    state_dir="''${PGDATA:-''${DEVENV_STATE:-.devenv/state}/postgres}"
    if [ -f "$state_dir/postmaster.pid" ]; then
      pid=$(head -n 1 "$state_dir/postmaster.pid" || true)
      if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        echo "Stopping leftover Postgres (pid $pid)"
        pg_ctl -D "$state_dir" stop -m fast || kill "$pid" || true
      fi
    fi
    exec start-postgres
  '';

  processes.valkey = {
    exec = ''
      # Valkey rewrites its cmdline to "valkey-server *:4202", so match the port.
      for pid in $(pgrep -x valkey-server || true); do
        cmd=$(tr '\0' ' ' < "/proc/$pid/cmdline" || true)
        case "$cmd" in
          *4202*)
            echo "Stopping leftover Valkey (pid $pid) on port 4202"
            kill "$pid" || true
            ;;
        esac
      done
      for _ in $(seq 1 50); do
        if ! { ss -H -ltn 'sport = :4202' 2>/dev/null || true; } | grep -q .; then
          break
        fi
        sleep 0.1
      done
      exec valkey-server --bind 127.0.0.1 --port 4202
    '';
  };

  processes.rustfs = {
    exec = ''
      mkdir -p "''${DEVENV_STATE:-.devenv/state}/rustfs"
      # The image runs as user rustfs and must be able to write the bind mount.
      chmod 777 "''${DEVENV_STATE:-.devenv/state}/rustfs"
      # A leftover container with this name (for example from a previous run) blocks docker run.
      docker rm -f lindaflor-rustfs >/dev/null 2>&1 || true
      exec docker run --rm --name lindaflor-rustfs \
        -p 127.0.0.1:4203:9000 \
        -p 127.0.0.1:4204:9001 \
        -v "''${DEVENV_STATE:-.devenv/state}/rustfs:/data" \
        -e RUSTFS_ACCESS_KEY=rustfsadmin \
        -e RUSTFS_SECRET_KEY=rustfsadmin \
        -e RUSTFS_ADDRESS=:9000 \
        -e RUSTFS_CONSOLE_ADDRESS=:9001 \
        -e RUSTFS_CONSOLE_ENABLE=true \
        rustfs/rustfs:latest /data
    '';
  };

  processes.db-studio = {
    exec = ''
      for _ in $(seq 1 50); do
        if psql "$DATABASE_URL" -c 'SELECT 1' >/dev/null 2>&1; then
          break
        fi
        sleep 0.2
      done
      exec pgweb --bind 127.0.0.1 --listen 4205 --skip-open --url "$DATABASE_URL"
    '';
  };

  processes.server = {
    exec = ''
      export PATH="''${DEVENV_ROOT}/.devenv/state/cargo-install/bin:''${PATH}"

      # sqlx::query! needs live schema at compile time — migrate before topcoat builds.
      echo "Waiting for Postgres..."
      for _ in $(seq 1 50); do
        if psql "$DATABASE_URL" -c 'SELECT 1' >/dev/null 2>&1; then
          break
        fi
        sleep 0.2
      done
      exec dev
    '';
  };

  # Compile against the live schema and rewrite .sqlx/ for rust-analyzer.
  scripts.dev.exec = ''
    set -euo pipefail
    export PATH="''${DEVENV_ROOT}/.devenv/state/cargo-install/bin:''${PATH}"
    migrate_log=$(sqlx migrate run 2>&1)
    if [ -n "$migrate_log" ]; then
      printf '%s\n' "$migrate_log"
    fi
    # rustc skips query macros when no Rust file changed, so a new migration
    # would leave .sqlx/ describing the old schema.
    if printf '%s\n' "$migrate_log" | grep -q '^Applied '; then
      touch src/lib.rs
    fi
    mkdir -p "''${DEVENV_ROOT}/.sqlx"
    export SQLX_OFFLINE=false
    export SQLX_OFFLINE_DIR="''${DEVENV_ROOT}/.sqlx"
    exec topcoat dev --bin lindaflor
  '';

  # Postgres up: migrate, compile online, refresh .sqlx/. Postgres down: use the cache.
  scripts.cargo-with-sqlx.exec = ''
    set -euo pipefail
    cache="''${DEVENV_ROOT}/.sqlx"
    mkdir -p "$cache"
    if psql "$DATABASE_URL" -c 'SELECT 1' >/dev/null 2>&1; then
      migrate_log=$(sqlx migrate run 2>&1)
      if [ -n "$migrate_log" ]; then
        printf '%s\n' "$migrate_log"
      fi
      if printf '%s\n' "$migrate_log" | grep -q '^Applied '; then
        touch src/lib.rs
      fi
      export SQLX_OFFLINE=false
      export SQLX_OFFLINE_DIR="$cache"
    else
      export SQLX_OFFLINE=true
    fi
    exec "$@"
  '';

  scripts.build.exec = ''
    export PATH="''${DEVENV_ROOT}/.devenv/state/cargo-install/bin:''${PATH}"
    cargo-with-sqlx cargo build --bin lindaflor
    topcoat asset bundle --bin lindaflor
  '';

  scripts.format = {
    exec = "cargo fmt";
    description = "Format Rust sources";
  };

  scripts.lint = {
    exec = "cargo-with-sqlx cargo clippy --all-targets";
    description = "Lint with clippy";
  };

  scripts.test = {
    exec = "cargo-with-sqlx cargo test";
    description = "Run tests (embedded Postgres)";
  };

  scripts.migrate = {
    exec = "sqlx migrate run";
    description = "Apply SQLx migrations";
  };

  scripts.prepare-sqlx = {
    exec = ''
      set -euo pipefail
      migrate_log=$(sqlx migrate run 2>&1)
      if [ -n "$migrate_log" ]; then
        printf '%s\n' "$migrate_log"
      fi
      if printf '%s\n' "$migrate_log" | grep -q '^Applied '; then
        touch src/lib.rs
      fi
      # Offline mode is the default. Preparing the cache has to reach Postgres.
      SQLX_OFFLINE=false cargo sqlx prepare -- --all-targets
    '';
    description = "Regenerate the sqlx offline query cache";
  };

  scripts.clean = {
    exec = "cargo clean";
    description = "Clean Cargo build artifacts";
  };

  scripts.ports.exec = ''
    echo ""
    echo "┌─────────────────────────────────────────────────────────┐"
    echo "│  topcoat full stack running                             │"
    echo "├─────────────────────────────────────────────────────────┤"
    echo "│  PostgreSQL: localhost:''${PGPORT:-4201}                │"
    echo "│  Valkey:     localhost:4202                             │"
    echo "│  RustFS S3:  localhost:4203                             │"
    echo "│  RustFS console: http://127.0.0.1:4204                  │"
    echo "│  App:        http://localhost:4200                      │"
    echo "│  Scalar:     http://localhost:4200/api/docs             │"
    echo "│  Swagger:    http://localhost:4200/api/swagger          │"
    echo "│  OpenAPI:    http://localhost:4200/api/openapi.json     │"
    echo "│  DB Studio:  http://127.0.0.1:4205                      │"
    echo "└─────────────────────────────────────────────────────────┘"
    echo ""
  '';

  scripts.db-reset.exec = ''
    set -euo pipefail
    echo "Stopping processes (if running)..."
    devenv processes down 2>/dev/null || true
    state_dir="''${DEVENV_STATE:-.devenv/state}/postgres"
    if [ -d "$state_dir" ]; then
      echo "Removing Postgres state at $state_dir"
      rm -rf "$state_dir"
    else
      echo "No Postgres state directory found at $state_dir"
    fi
    echo "Done. Run 'devenv up' (server process runs sqlx migrate before build)."
  '';

  scripts.seed.exec = ''
    cargo-with-sqlx cargo run --bin seed
  '';

  enterShell = ''
    export PATH="''${DEVENV_ROOT}/.devenv/state/cargo-install/bin:''${PATH}"
    echo "lindaflor devenv"
    echo "  rustc: $(rustc --version 2>/dev/null || echo unavailable)"
    echo "  Start stack: devenv up"
    echo "  App only:   devenv shell dev"
    echo "  Build:      devenv shell build"
    echo "  Clean:      devenv shell clean"
    echo "  Format:     devenv shell format"
    echo "  Lint:       devenv shell lint"
    echo "  Test:       devenv shell test"
    echo "  Migrate:    devenv shell migrate"
    echo "  SQLx cache: devenv shell prepare-sqlx"
    echo "  Seed database: devenv shell seed"
    echo "  Show ports: devenv shell ports"
    echo "  Reset DB:   devenv shell db-reset"
    echo "  DB Studio: http://127.0.0.1:4205 (via devenv up)"
  '';
}
