# Build the openterface-rs binary on NixOS without the flake, using a pinned
# source revision. This is the reference derivation for integrating openterface-rs
# into a system configuration (e.g. a microVM), and the template that replaces a
# CMake/C++ `openterface` derivation.
#
# winit + wgpu dlopen Wayland + Vulkan at runtime, so the binary MUST be wrapped
# with LD_LIBRARY_PATH (a plain rpath is not enough for dlopen).
#
# Usage:
#   openterface-rs = pkgs.callPackage ./packaging/nixos/openterface-rs.nix {
#     rev = "<git rev>";
#     hash = "<sha256 of the source>";
#     cargoHash = "<vendored cargo deps hash>";
#   };
{ lib
, rustPlatform
, fetchFromGitHub
, pkg-config
, makeWrapper
, clang
, udev
, libv4l
, wayland
, wayland-protocols
, libxkbcommon
, libdecor
, vulkan-loader
, libGL
, rev ? "main"
, hash ? lib.fakeHash
, cargoHash ? lib.fakeHash
}:

let
  runtimeLibs = [
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
rustPlatform.buildRustPackage {
  pname = "openterface-rs";
  version = "1.0.0-unstable-${builtins.substring 0 7 rev}";

  src = fetchFromGitHub {
    owner = "vicondoa";
    repo = "openterface-rs";
    inherit rev hash;
  };

  inherit cargoHash;

  # Ship the real-hardware path.
  buildAndTestSubdir = "crates/openterface-cli";
  buildFeatures = [ "hardware" ];
  # The Nix sandbox has no GPU/Wayland; the hardware-free tests pass but the
  # display/gpu lanes are covered by CI (lavapipe), so skip checks here.
  doCheck = false;

  nativeBuildInputs = [ pkg-config makeWrapper clang ];
  buildInputs = runtimeLibs;

  LIBCLANG_PATH = "${clang.cc.lib}/lib";

  postInstall = ''
    wrapProgram $out/bin/openterface-rs \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}"
    install -Dm0644 packaging/udev/60-openterface.rules \
      $out/lib/udev/rules.d/60-openterface.rules
  '';

  meta = with lib; {
    description = "Native-Linux, Wayland-only, Qt-free Rust port of the Openterface Mini-KVM";
    homepage = "https://github.com/vicondoa/openterface-rs";
    license = licenses.asl20;
    platforms = platforms.linux;
    mainProgram = "openterface-rs";
  };
}
