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

The flake also exports a NixOS module:

```nix
{
  inputs.openterface-rs.url = "github:vicondoa/openterface-rs";

  outputs = { self, nixpkgs, openterface-rs, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        openterface-rs.nixosModules.default
        {
          programs.openterface-rs = {
            enable = true;
            paste.shortcut = "ctrl-shift-v";
            paste.middleClick = "off";      # "primary" or "clipboard" to paste on middle-click
            paste.maxChars = 4096;
          };
        }
      ];
    };
  };
}
```

The module installs a wrapper package and can set every documented runtime
configuration value:

| Module option | Runtime variable |
|---------------|------------------|
| `logFilter` | `RUST_LOG` |
| `titlePrefix` | `OPENTERFACE_TITLE_PREFIX` |
| `mouseIntervalMs` | `OPENTERFACE_MOUSE_INTERVAL_MS` |
| `throttle.enable` | `OPENTERFACE_THROTTLE` |
| `throttle.idleDecodeMs` | `OPENTERFACE_IDLE_DECODE_MS` |
| `throttle.inputWakeMs` | `OPENTERFACE_INPUT_WAKE_MS` |
| `throttle.idleWatchdogMs` | `OPENTERFACE_IDLE_WATCHDOG_MS` |
| `fullscreen` | `OPENTERFACE_FULLSCREEN` |
| `useLibdecor` | `OPENTERFACE_USE_LIBDECOR` |
| `requireGpu` | `OPENTERFACE_REQUIRE_GPU` |
| `paste.enable` | `OPENTERFACE_ENABLE_PASTE` |
| `paste.shortcut` | `OPENTERFACE_PASTE_SHORTCUT` |
| `paste.middleClick` | `OPENTERFACE_MIDDLE_CLICK_PASTE` |
| `paste.maxChars` | `OPENTERFACE_PASTE_MAX_CHARS` |

`installUdevRules = true` (default) adds the package to
`services.udev.packages`; `createGroup = true` (default) creates the
`openterface` group for headless/non-seat fallback access.

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
