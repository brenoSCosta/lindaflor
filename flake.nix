{
  description = "lindaflor toolchain (Rust 1.98.1)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable."1.98.1".default.override {
          extensions = [
            "rust-src"
            "clippy"
            "rustfmt"
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            sqlx-cli
            valkey
            postgresql
            pkg-config
            openssl
            cargo-watch
            cargo-edit
          ];
          shellHook = ''
            echo "lindaflor flake shell (rustc $(rustc --version))"
            echo "Start valkey with: valkey-server --port 4202"
            echo "Run migrations with: sqlx migrate run"
          '';
        };
      }
    );
}
