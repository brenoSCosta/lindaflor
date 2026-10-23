{
  description = "lindaflor — Go 1.27 + templ + htmx + Tailwind v4";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;

        # Pin the toolchain to Go 1.27.3. nixos-unstable already ships it; the
        # override only kicks in if the channel ever regresses to another patch.
        goVersion = "1.27.3";
        go =
          if pkgs.go_1_27.version == goVersion then
            pkgs.go_1_27
          else
            pkgs.go_1_27.overrideAttrs (_: {
              version = goVersion;
              src = pkgs.fetchurl {
                url = "https://go.dev/dl/go${goVersion}.src.tar.gz";
                sha256 = "6e7749035cf360f06b2575d162ea79df9fda8ef671889e5c375f8e50777f26bb";
              };
            });

        buildGoModule = pkgs.buildGoModule.override { inherit go; };

        tools = with pkgs; [
          go
          templ
          sqlc
          goose
          golangci-lint
          gofumpt
          gotools
          gopls
          air
          tailwindcss_4
          postgresql_17
          valkey
          pgweb
          nixfmt
          gnumake
          govulncheck
          hadolint
          actionlint
          shellcheck
        ];
      in
      {
        formatter = pkgs.nixfmt-tree;

        packages.default = buildGoModule {
          pname = "lindaflor";
          version = "0.1.0";
          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./go.mod
              ./go.sum
              ./cmd
              ./internal
              ./db
              ./web
            ];
          };
          # Update with `nix build 2>&1 | grep got:` after changing go.mod.
          vendorHash = "sha256-7jNhhlAzxrc9v2Fqs5pODcwFwwFAKG1sfTUwzSwVDaA=";
          subPackages = [
            "cmd/server"
            "cmd/seed"
            "cmd/create-admin"
          ];
          env.CGO_ENABLED = "0";
          ldflags = [
            "-s"
            "-w"
          ];
          # Integration tests need Postgres; `go test` runs in CI/devShell instead.
          doCheck = false;
          meta.mainProgram = "server";
        };

        devShells.default = pkgs.mkShell {
          packages = tools;
          # cgo stays enabled here so `go test -race` works (mkShell ships a C compiler).
          env.GOTOOLCHAIN = "local";
          shellHook = ''
            echo "lindaflor flake shell ($(go version))"
            echo "  make generate | make dev | make test | make lint"
          '';
        };
      }
    );
}
