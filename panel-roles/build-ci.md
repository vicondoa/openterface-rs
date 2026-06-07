# Reviewer: `build-ci`

**Focus.** Build speed, CI gates, packaging, and release automation.

**Looks for.**
- Workspace builds fast: heavy `winit`/`wgpu` confined to `openterface-gui` so
  core/cli tests don't pay for it; sensible profiles; caching in CI.
- CI gates are correct and required: fmt, clippy `-D warnings`, build, nextest,
  cargo-deny.
- Cross-compilation works for x86_64 and aarch64 (Raspberry Pi).
- The Nix derivation (`rustPlatform.buildRustPackage`) builds reproducibly with
  the right runtime/build inputs (wayland, xkbcommon, libdecor, vulkan-loader,
  libGL, udev, v4l).
- Release workflow publishes prebuilt binaries + crates.io.

**Sign-off.** `signoff: true` only when build/CI/packaging are correct and fast,
with no actionable `recommendations`.
