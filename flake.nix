{
  description = "openterface-rs — native-Linux, Wayland-only Rust host application for the Openterface Mini-KVM";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    (flake-utils.lib.eachDefaultSystem (system:
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

        nixosModuleCheck =
          let
            fakePackage = pkgs.runCommand "openterface-rs-fake" {
              pname = "openterface-rs";
              meta.mainProgram = "openterface-rs";
            } ''
              mkdir -p "$out/bin" "$out/lib/udev/rules.d"
              cat > "$out/bin/openterface-rs" <<'EOF'
              #!${pkgs.runtimeShell}
              echo fake openterface-rs "$@"
              EOF
              chmod +x "$out/bin/openterface-rs"
              echo '# fake udev rules' > "$out/lib/udev/rules.d/60-openterface.rules"
            '';
            systemConfig = nixpkgs.lib.nixosSystem {
              inherit system;
              modules = [
                (import ./packaging/nixos/module.nix { inherit self; })
                ({ ... }: {
                  system.stateVersion = "25.11";
                  boot.loader.grub.enable = false;
                  documentation.nixos.enable = false;
                  environment.defaultPackages = [ ];
                  fileSystems."/" = {
                    device = "none";
                    fsType = "tmpfs";
                  };
                  programs.openterface-rs = {
                    enable = true;
                    package = fakePackage;
                    logFilter = "info";
                    titlePrefix = "[test] ";
                    mouseIntervalMs = 50;
                    throttle = {
                      enable = false;
                      idleDecodeMs = 125;
                      inputWakeMs = 300;
                      idleWatchdogMs = 1500;
                    };
                    fullscreen = true;
                    useLibdecor = false;
                    windowMaxSize = "1920x1080";
                    captureSizing = "fixed";
                    requireGpu = true;
                    paste = {
                      enable = false;
                      shortcut = "ctrl-alt-shift-v";
                      middleClick = "clipboard";
                      maxChars = 1234;
                    };
                    extraEnvironment.OPENTERFACE_EXTRA_TEST = "ok";
                  };
                })
              ];
            };
            configuredPackage = builtins.head systemConfig.config.environment.systemPackages;
            udevPackage = builtins.head systemConfig.config.services.udev.packages;
          in
          pkgs.runCommand "openterface-rs-nixos-module-check" { } ''
            set -eu
            wrapper=${configuredPackage}/bin/openterface-rs
            test -x "$wrapper"
            grep -q 'RUST_LOG' "$wrapper"
            grep -q 'OPENTERFACE_TITLE_PREFIX' "$wrapper"
            grep -q 'OPENTERFACE_MOUSE_INTERVAL_MS' "$wrapper"
            grep -q 'OPENTERFACE_THROTTLE' "$wrapper"
            grep -q 'OPENTERFACE_IDLE_DECODE_MS' "$wrapper"
            grep -q 'OPENTERFACE_INPUT_WAKE_MS' "$wrapper"
            grep -q 'OPENTERFACE_IDLE_WATCHDOG_MS' "$wrapper"
            grep -q 'OPENTERFACE_FULLSCREEN' "$wrapper"
            grep -q 'OPENTERFACE_USE_LIBDECOR' "$wrapper"
            grep -q 'OPENTERFACE_WINDOW_MAX_SIZE' "$wrapper"
            grep -q 'OPENTERFACE_CAPTURE_SIZING' "$wrapper"
            grep -q 'OPENTERFACE_REQUIRE_GPU' "$wrapper"
            grep -q 'OPENTERFACE_ENABLE_PASTE' "$wrapper"
            grep -q 'OPENTERFACE_PASTE_SHORTCUT' "$wrapper"
            grep -q 'OPENTERFACE_MIDDLE_CLICK_PASTE' "$wrapper"
            grep -q 'OPENTERFACE_PASTE_MAX_CHARS' "$wrapper"
            grep -q 'OPENTERFACE_EXTRA_TEST' "$wrapper"
            test -f ${udevPackage}/lib/udev/rules.d/60-openterface.rules
            test '${if systemConfig.config.users.groups ? openterface then "yes" else "no"}' = yes
            echo ok > "$out"
          '';
      in
      {
        packages.default = let prebuilt = import ./nix/prebuilt.nix { inherit pkgs; }; in if prebuilt != null then prebuilt else openterface-rs;
        packages.openterface-rs = openterface-rs;

        checks.nixos-module = nixosModuleCheck;

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
      }) // {
        nixosModules.default = import ./packaging/nixos/module.nix { inherit self; };
        nixosModules.openterface-rs = self.nixosModules.default;
      });
}
