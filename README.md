# openterface-rs

A native-Linux, Wayland-only, Qt-free reimplementation of the
[Openterface Mini-KVM](https://openterface.com) host application, written entirely
in **Rust**.

openterface-rs lets you control a target computer's **keyboard, video, and mouse**
over a single USB connection — no network required — using the Openterface
hardware (MS2109 HDMI capture + CH9329 USB-serial HID bridge).

> **Status:** v1.0 targets **core-KVM parity** with the C++ CLI — video plus
> keyboard/mouse over one USB cable. Real-hardware validation runs in a VM via
> the closed-loop harness. See [`PLAN.md`](PLAN.md) for the roadmap and
> [`docs/`](docs/README.md) for full documentation.

## Why a Rust port?

- **Fast builds, lean dependencies, no Qt.** A device-agnostic core library
  plus a thin CLI and an optional `winit`/`wgpu` display frontend.
- **Testable without hardware.** Every hardware interaction is a trait, so the
  full pipeline runs against simulated devices — the test-suite needs no device.
- **Linux + Wayland native.** No XWayland.

## Install

Prebuilt binaries for **x86_64** and **aarch64** (64-bit Pi):

```bash
curl -fsSL https://raw.githubusercontent.com/vicondoa/openterface-rs/main/packaging/install.sh -o install.sh
sh install.sh                         # verifies its own download checksums
```

Or with Cargo / Nix:

```bash
cargo install --git https://github.com/vicondoa/openterface-rs openterface-cli --features hardware --locked
nix run github:vicondoa/openterface-rs
```

See the [install guide](docs/how-to/install.md) and
[permissions & udev](docs/how-to/permissions-udev.md) for non-root device
access.

## Usage

```bash
openterface-rs scan        # list Openterface devices
openterface-rs status      # show device status
openterface-rs connect     # open the KVM session (auto-detects)
openterface-rs reset --serial /dev/ttyACM0   # CH9329 factory reset
```

Runtime behavior is tunable via `OPENTERFACE_*` environment variables (mouse
pacing, idle-decode throttling, fullscreen) — see the
[CLI reference](docs/reference/cli.md) and
[environment variables](docs/reference/env-vars.md).

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
cargo test --workspace        # runs with no hardware, no system libraries
```

A Nix dev shell (toolchain + system libraries) is provided:

```bash
nix develop
```

See the [build guide](docs/how-to/build.md) for the hardware features and the
full set of CI gates.

## Hardware

| Endpoint | Chip | Linux node | Carries |
|----------|------|------------|---------|
| Video | MS2109 HDMI capture | `/dev/videoN` (UVC / MJPEG) | the target's screen |
| Input | CH9329 USB-serial HID | `/dev/ttyACM0` (115200 8N1) | mouse + keyboard |

## Documentation

Full docs live in [`docs/`](docs/README.md): the
[CLI reference](docs/reference/cli.md),
[CH9329 protocol](docs/reference/protocol.md),
[environment variables](docs/reference/env-vars.md),
[install](docs/how-to/install.md) / [udev](docs/how-to/permissions-udev.md) /
[troubleshooting](docs/how-to/troubleshooting.md) guides, and the
[architecture](ARCHITECTURE.md) explanation.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).

This is an independent reimplementation. It implements the same non-copyrightable
hardware protocols (CH9329, MS2109) as the C++ Openterface CLIs; it is not a
derivative of, and copies no source from, the GPL/AGPL Qt application.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md) (development
process, panel review, versioning, and test layout).
