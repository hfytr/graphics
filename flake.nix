{
  inputs = {
    nixpkgs.url      = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url  = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay?ref=snapshot/2025-01-11";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        rustpkg-old = pkgs.rust-bin.fromRustupToolchainFile ./shaders/rust-toolchain.toml;
        # this is nightly 2025-01-01
        rustpkg = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.default.override {
          extensions = [ "rust-src" "rust-analyzer" "rustfmt" "rustc-dev" "llvm-tools-preview" ];
          targets = [ "arm-unknown-linux-gnueabihf" ];
        });
        rustDevShell = rust: with pkgs; mkShell rec {
          nativeBuildInputs = [
            pkg-config
            wayland
            libxkbcommon
            vulkan-headers
            vulkan-tools
            vulkan-loader
            vulkan-validation-layers
            spirv-tools
            renderdoc
            xorg.libX11
            rust
          ];
          LD_LIBRARY_PATH = "${lib.makeLibraryPath nativeBuildInputs}";
        };
      in {
        RUST_BACKTRACE = 1;
        CARGO_PROFILE_DEV_BUILD_OVERRIDE_DEBUG=1;
        devShells.cpu = rustDevShell rustpkg;
        devShells.shader = rustDevShell rustpkg-old;
      }
    );
}
