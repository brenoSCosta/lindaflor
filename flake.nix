{
  description = "topcoat dev environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustc
            cargo
            sqlx-cli
            valkey
            postgresql
            pkg-config
            openssl
            cargo-watch
            cargo-edit
          ];
          shellHook = ''
            echo "topcoat dev shell"
            echo "Start valkey with: valkey-server --port 4202"
            echo "Run migrations with: sqlx migrate run"
          '';
        };
      });
}
