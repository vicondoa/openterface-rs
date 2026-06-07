{
  description = "openterface-rs — native-Linux, Wayland-only Rust port of the Openterface Mini-KVM";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        # Runtime/build system libraries for the workspace. The GUI crate
        # (winit + wgpu) needs Wayland, xkbcommon, and a Vulkan loader; the
        # core needs udev + v4l.
        nativeBuildInputs = with pkgs; [ pkg-config ];
        buildInputs = with pkgs; [
          udev
          libv4l
          wayland
          wayland-protocols
          libxkbcommon
          libdecor
          vulkan-loader
          libGL
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            cargo-nextest
            cargo-deny
          ];
          # winit/wgpu dlopen Wayland + Vulkan at runtime.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        };
      });
}
