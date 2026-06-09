{
  description = "openterface-rs — native-Linux, Wayland-only Rust host application for the Openterface Mini-KVM";

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

        # winit + wgpu dlopen Wayland + Vulkan at runtime, so the installed
        # binary must find them via LD_LIBRARY_PATH (a plain rpath is not
        # enough for dlopen). This wrapper is also the basis for the W6
        # /etc/nixos work-ssd derivation.
        openterface-rs = pkgs.rustPlatform.buildRustPackage {
          pname = "openterface-rs";
          version = (builtins.fromTOML
            (builtins.readFile ./Cargo.toml)).workspace.package.version;
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = nativeBuildInputs ++ [ pkgs.makeWrapper pkgs.clang ];
          inherit buildInputs;

          # Ship the real-hardware path.
          buildAndTestSubdir = "crates/openterface-cli";
          buildFeatures = [ "hardware" ];
          # The default `cargo test` is hardware-free, but the Nix sandbox has no
          # GPU/Wayland; keep the package build deterministic by not running the
          # display/gpu tests here (CI covers them on lavapipe).
          doCheck = false;

          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";

          postInstall = ''
            wrapProgram $out/bin/openterface-rs \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath buildInputs}"
            install -Dm0644 packaging/udev/60-openterface.rules \
              $out/lib/udev/rules.d/60-openterface.rules
          '';

          meta = with pkgs.lib; {
            description = "Native-Linux, Wayland-only Rust host application for the Openterface Mini-KVM";
            homepage = "https://github.com/vicondoa/openterface-rs";
            license = licenses.asl20;
            platforms = platforms.linux;
            mainProgram = "openterface-rs";
          };
        };
      in
      {
        packages.default = openterface-rs;
        packages.openterface-rs = openterface-rs;

        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            cargo-nextest
            cargo-deny
            # bindgen (v4l2-sys) needs libclang.
            clang
          ];
          LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
          # winit/wgpu dlopen Wayland + Vulkan at runtime.
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
        };
      });
}
