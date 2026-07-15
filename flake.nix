{
  description = "miku — filesystem-owned personal Markdown wiki";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            # rust toolchain
            rustc
            cargo
            rustfmt
            clippy
            # python scripting/automation (run via `uv run`, replaces bash glue)
            uv
            bun
            # HTTP smoke/load probe used by make bench when a server is running
            oha
            # Repeatable command and request timing comparisons.
            hyperfine
            # formatting
            prettier
            # local Postgres for the native (no-container) dev stack — see
            # `make db-up` / `make dev`. The index is disposable; runs against a
            # project-local cluster under .pgdata (gitignored).
            postgresql_18
          ];
        };
      });
}
