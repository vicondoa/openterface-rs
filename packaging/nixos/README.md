# Running openterface-rs on NixOS

Two ways to build, plus the integration template for a system/microVM config.

## 1. Flake (simplest)

```bash
nix run github:vicondoa/openterface-rs              # run
nix build github:vicondoa/openterface-rs            # ./result/bin/openterface-rs
```

The flake's `packages.default` is a `rustPlatform.buildRustPackage` built with
the `hardware` feature, wrapped so the winit/wgpu `dlopen` libraries (Wayland,
Vulkan, xkbcommon, libdecor, GL) resolve at runtime, and it ships the udev rules
under `$out/lib/udev/rules.d`.

## 2. Pinned derivation (no flake)

[`openterface-rs.nix`](openterface-rs.nix) builds from a pinned source revision
via `fetchFromGitHub`. Fill in `rev`, `hash`, and `cargoHash`:

```nix
openterface-rs = pkgs.callPackage ./packaging/nixos/openterface-rs.nix {
  rev = "<git rev>";
  hash = "<nix-prefetch source hash>";
  cargoHash = "<vendored cargo deps hash>";   # build once with lib.fakeHash to learn it
};
```

## 3. System integration (udev + group)

```nix
{ pkgs, ... }:
{
  environment.systemPackages = [ openterface-rs ];
  services.udev.packages = [ openterface-rs ];   # installs 60-openterface.rules
  users.groups.openterface = { };
  users.users.<you>.extraGroups = [ "openterface" ];
}
```

On a Wayland seat the rules' `uaccess` tag grants the logged-in user access; for
headless/SSH use, the `openterface` group is the fallback.

## 4. System or microVM integration notes

The runtime contract is:

- The binary is `openterface-rs`; it accepts `connect --video=<path> --serial=<path>`
  (compatible with a wrapper that auto-detects the nodes).
- The `OPENTERFACE_*` runtime tunables (mouse pacing, idle-decode throttle,
  fullscreen) are honored — see [`docs/reference/env-vars.md`](../../docs/reference/env-vars.md).

Before switching a production KVM over, run the
[closed-loop harness](../../docs/how-to/closed-loop-harness.md) against the real
device.
