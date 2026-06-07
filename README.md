# openterface-rs

A native-Linux, Wayland-only, Qt-free reimplementation of the
[Openterface Mini-KVM](https://openterface.com) host application, written entirely
in **Rust**.

openterface-rs lets you control a target computer's **keyboard, video, and mouse**
over a single USB connection — no network required — using the Openterface
hardware (MS2109 HDMI capture + CH9329 USB-serial HID bridge).

> **Status:** early development. See [`PLAN.md`](PLAN.md) for the wave-by-wave
> implementation roadmap and current progress.

## Why a Rust port?

- **Fast builds, lean dependencies, no Qt.** A device-agnostic core library
  plus a thin CLI and an optional `winit`/`wgpu` display frontend.
- **Testable without hardware.** Every hardware interaction is a trait, so the
  full pipeline runs against simulated devices — the test-suite needs no device.
- **Linux + Wayland native.** No XWayland.

## Workspace layout

| Crate | Role |
|-------|------|
| `openterface-core` | Device-agnostic core: CH9329 protocol, V4L2 capture, decode, discovery, input, pacing, session. No GUI, no async runtime. |
| `openterface-cli` | The `openterface-rs` command-line frontend. |
| `openterface-gui` | Native Wayland display: `winit` + `wgpu`. |
| `openterface-test-support` | Simulated devices and fixtures for hardware-free tests. |

## Building

```bash
cargo build --workspace
cargo test --workspace        # runs with no hardware
```

A Nix dev shell is provided:

```bash
nix develop
```

## Hardware

| Endpoint | Chip | Linux node | Carries |
|----------|------|------------|---------|
| Video | MS2109 HDMI capture | `/dev/videoN` (UVC / MJPEG) | the target's screen |
| Input | CH9329 USB-serial HID | `/dev/ttyACM0` (115200 8N1) | mouse + keyboard |

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

This is an independent reimplementation. It implements the same non-copyrightable
hardware protocols (CH9329, MS2109) as the C++ Openterface CLIs; it is not a
derivative of, and copies no source from, the GPL/AGPL Qt application.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md) (development
process, panel review, versioning, and test layout).
