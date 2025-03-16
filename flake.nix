{
  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustpkg = pkgs.rust-bin.nightly."2024-11-22".default.override {
          extensions = [ "rust-src" "rust-analyzer" "rustfmt" "llvm-tools"];
          targets = [ "x86_64-unknown-linux-gnu" ];
        };
      in {
        RUST_BACKTRACE = 1;
        devShells.default = with pkgs; mkShell rec {
          nativeBuildInputs = [
            pkg-config
            rustpkg
            shaderc
            wayland
            libxkbcommon
            vulkan-headers
            vulkan-tools
            vulkan-loader
            vulkan-validation-layers
          ];
          LD_LIBRARY_PATH = "${lib.makeLibraryPath nativeBuildInputs}";
        };
      }
    );
}
