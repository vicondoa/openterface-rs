# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.1] - 2026-06-08

### Fixed
- **On-screen video showed a black window with real hardware.** The capture
  thread polled the V4L2 backend with a 10 ms timeout, but the backend rebuilds
  the MMAP stream on every timed-out dequeue (a workaround for a v4l buffer-
  bookkeeping wedge). At 1080p MJPG the first frame after `STREAMON` takes longer
  than 10 ms, so the stream was perpetually torn down and rebuilt and **no frame
  ever reached the renderer**. The capture loop now waits up to 1 s for a frame,
  so a normal dequeue is no longer mistaken for a stall. The hardware-free tests
  missed this because the simulated source delivers frames instantly; a new
  `capture_loop_survives_first_frame_warmup` regression test now models a
  device with first-frame warm-up and fails if the timeout is ever shortened.

### Added
- Gated `tracing` diagnostics on the capture → decode → GPU-upload path (first
  frame, periodic frame counts, capture stalls, and decode failures), which were
  previously silent — making future black-window regressions diagnosable.

## [1.0.0] - 2026-06-08

openterface-rs is a native-Linux, Wayland-only, Qt-free Rust reimplementation of
the Openterface Mini-KVM host CLI, at **core-KVM parity** with the C++ CLI (video
+ keyboard/mouse over one USB cable). Hardware-validated in the work-ssd VM
(device discovery, live capture, and CH9329 injection moving the target cursor).

### Added
- **CLI** (`openterface-rs`): `connect` (with `--video`/`--serial`/`--no-serial`/
  `--no-video` (input-only)/`--dummy`/`--debug`), `scan` (enumerates all video +
  serial nodes), `status`, and `reset --serial` (RTS factory reset +
  reconfigure), with `-v/--verbose`, `--version`, and `RUST_LOG` logging.
- **CH9329 input**: framed command builders (absolute/relative mouse, keyboard +
  modifiers, Ctrl+Alt+Del, software/factory reset) with golden-vector tests;
  baud fallback (115200 → 9600); tolerant of `GET_INFO`-silent firmware.
- **V4L2 video**: MJPEG (`zune-jpeg`) and YUYV decode; capture configuration;
  auto-selection of the `uvcvideo` + `MJPG` node (skips the virtio-media adapter).
- **Native Wayland display**: `winit` + `wgpu` window, client-side decorations
  for tiling/CSD compositors (niri), window→absolute coordinate mapping, and
  winit-key→HID input forwarding.
- **Load-bearing behaviors**: ~30 Hz CH9329 mouse-move pacing with release
  priority (`OPENTERFACE_MOUSE_INTERVAL_MS`); idle MJPEG-decode throttling with
  input wake and anti-freeze watchdog (`OPENTERFACE_THROTTLE`,
  `OPENTERFACE_IDLE_DECODE_MS`, `OPENTERFACE_INPUT_WAKE_MS`,
  `OPENTERFACE_IDLE_WATCHDOG_MS`); `OPENTERFACE_FULLSCREEN`.
- **Hardware-free test suite**: every device interaction is a trait, tested
  against simulators + a PTY round-trip + a headless wgpu render test; the
  default `cargo test --workspace` needs no hardware and no system libraries.
- **Closed-loop harness** (`tools/kvm-debug.sh`): non-destructive automated
  device verification (framing, frame liveness, mouse-move pixel-diff) with a
  hardware-free framing-drift test guarding it.
- **Packaging**: least-privilege udev rules (`uaccess` + dedicated group),
  checksum-verifying `install.sh`, a release workflow for x86_64 + aarch64, a
  Nix flake package, and crates.io metadata.
- **Docs**: a Diataxis `docs/` tree (CLI/protocol/device-id/env-var reference;
  install/udev/build/troubleshooting/harness how-tos; architecture, threading,
  and testing explanations), plus README, ARCHITECTURE, CONTRIBUTING, SECURITY,
  and AGENTS.
- Initial Cargo workspace and crate scaffold (`openterface-core`,
  `openterface-cli`, `openterface-gui`, `openterface-test-support`) with the
  device-agnostic interface contracts (`SerialTransport`, `VideoSource`,
  `DeviceScanner`).

### Known deviations from the C++ CLI
- Runtime failures exit **`1`** (not the C++ `0`); usage errors exit `2`.
- `status` is detection-based rather than in-process connection state.
- No auto "GUI-only" window when no device is found: `connect` errors instead
  (use `--dummy` for a deviceless window).
- `OPENTERFACE_USE_LIBDECOR=0` falls back to a bare xdg-shell window; full
  CSD/SSD negotiation parity beyond that is best-effort under winit.

[Unreleased]: https://github.com/vicondoa/openterface-rs/compare/v1.0.1...HEAD
[1.0.1]: https://github.com/vicondoa/openterface-rs/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/vicondoa/openterface-rs/releases/tag/v1.0.0
