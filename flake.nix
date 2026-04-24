{
  description = "oxidicom env";
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };
  outputs =
    { flake-utils, nixpkgs, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
      in
      {
        devShells.default = with pkgs; mkShell {
          buildInputs = [
            gcc14
            rust-bin.stable.latest.default
          ];
        };
      }
    );
}
