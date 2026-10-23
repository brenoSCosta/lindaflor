{
  pkgs,
  lib,
  config,
  ...
}:

let
  # Go 1.27.3 (same toolchain as flake.nix).
  go = pkgs.go_1_27;
  generatedFiles = [
    "^web/static/vendor/"
    "^web/static/css/app\\.css$"
    "^internal/db/sqlc/"
    "_templ\\.go$"
  ];
in
{
  packages = with pkgs; [
    templ
    sqlc
    goose
    golangci-lint
    gofumpt
    gotools
    gopls
    air
    tailwindcss_4
    pgweb
    valkey
    nixfmt
    gnumake
    govulncheck
    hadolint
    actionlint
    shellcheck
  ];

  languages.go = {
    enable = true;
    package = go;
  };

  env.GOTOOLCHAIN = "local";
  env.DATABASE_URL = "postgres://postgres:postgres@127.0.0.1:4201/lindaflor?sslmode=disable";
  env.PGPORT = lib.mkForce "4201";
  env.VALKEY_URL = "redis://127.0.0.1:4202";
  env.S3_ENDPOINT = "http://127.0.0.1:4203";
  env.S3_REGION = "us-east-1";
  env.S3_ACCESS_KEY_ID = "rustfsadmin";
  env.S3_SECRET_ACCESS_KEY = "rustfsadmin";
  env.S3_BUCKET = "lindaflor";
  env.PORT = "4200";
  env.APP_ENV = "development";
  env.APP_ORIGIN = "http://localhost:4200";
  env.GOOGLE_CLIENT_ID = "fake-client.apps.googleusercontent.com";
  env.GOOGLE_CLIENT_SECRET = "GOCSPX-fake";
  env.TOKEN_PEPPER = "dev-pepper-change-me";
  env.LOG_SAMPLE_RATE = "1";
  env.LOG_SLOW_THRESHOLD_MS = "1000";

  git-hooks.hooks = {
    check-yaml.enable = true;
    check-merge-conflicts.enable = true;
    # Vendored and generated files are committed byte-for-byte.
    end-of-file-fixer = {
      enable = true;
      excludes = generatedFiles;
    };
    trim-trailing-whitespace = {
      enable = true;
      excludes = generatedFiles;
    };
    nixfmt.enable = true;

    go-fmt = {
      enable = true;
      name = "gofumpt + goimports (golangci-lint fmt)";
      entry = "golangci-lint fmt --diff";
      files = "\\.go$";
      pass_filenames = false;
    };
    templ-fmt = {
      enable = true;
      name = "templ fmt";
      entry = "templ fmt -fail .";
      files = "\\.templ$";
      pass_filenames = false;
    };
    golangci-lint = {
      enable = true;
      name = "golangci-lint";
      entry = "golangci-lint run ./...";
      files = "\\.(go|templ)$";
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
        name = "lindaflor";
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

  processes.valkey.exec = ''
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
    mkdir -p "''${DEVENV_STATE:-.devenv/state}/valkey"
    exec valkey-server --bind 127.0.0.1 --port 4202 \
      --dir "''${DEVENV_STATE:-.devenv/state}/valkey" \
      --appendonly yes --appendfsync everysec \
      --save 900 1 --save 300 10 --save 60 10000
  '';

  processes.rustfs.exec = ''
    mkdir -p "''${DEVENV_STATE:-.devenv/state}/rustfs"
    # The image runs as user rustfs and must be able to write the bind mount.
    chmod 777 "''${DEVENV_STATE:-.devenv/state}/rustfs"
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

  processes.db-studio.exec = ''
    for _ in $(seq 1 50); do
      pg_isready -h 127.0.0.1 -p "''${PGPORT:-4201}" -U postgres -d lindaflor -t 1 >/dev/null 2>&1 && break
      sleep 0.2
    done
    exec pgweb --bind 127.0.0.1 --listen 4205 --skip-open --url "$DATABASE_URL"
  '';

  processes.server.exec = ''
    echo "Waiting for Postgres..."
    for _ in $(seq 1 50); do
      pg_isready -h 127.0.0.1 -p "''${PGPORT:-4201}" -U postgres -d lindaflor -t 1 >/dev/null 2>&1 && break
      sleep 0.2
    done
    exec dev
  '';

  # templ + sqlc + tailwind (generated code is committed; CI checks drift).
  scripts.generate = {
    exec = "make generate";
    description = "Generate templ, sqlc and Tailwind outputs";
  };

  # Live reload; the server applies migrations on boot in development.
  scripts.dev = {
    exec = "air";
    description = "Run the app with live reload (air)";
  };

  scripts.lint = {
    exec = "make lint";
    description = "golangci-lint + sqlc vet + templ fmt check";
  };

  scripts.fmt = {
    exec = "make fmt";
    description = "Format Go (gofumpt + goimports) and templ";
  };

  scripts.test = {
    exec = ''go test "''${@:-./...}"'';
    description = "Run tests (nix Postgres + miniredis)";
  };

  scripts.migrate = {
    exec = "make migrate";
    description = "Apply goose migrations to $DATABASE_URL";
  };

  scripts.seed = {
    exec = "make seed";
    description = "Seed the development database";
  };

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
    echo "Done. Run 'devenv up' (the server migrates on boot in development)."
  '';

  scripts.ports.exec = ''
    echo ""
    echo "  PostgreSQL:     localhost:''${PGPORT:-4201} (db lindaflor)"
    echo "  Valkey:         localhost:4202"
    echo "  RustFS S3:      localhost:4203 (console http://127.0.0.1:4204)"
    echo "  DB Studio:      http://127.0.0.1:4205"
    echo "  App:            http://localhost:4200"
    echo ""
  '';

  enterShell = ''
    echo "lindaflor devenv ($(go version))"
    echo "  devenv up            start postgres/valkey/rustfs/pgweb + app (air)"
    echo "  generate | dev | lint | fmt | test | migrate | seed | db-reset | ports"
  '';
}
