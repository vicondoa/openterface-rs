# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Dependabot configuration for weekly GitHub Actions updates with a 14-day
  cooldown.
- Focused GUI paste: `Ctrl+Shift+V` reads the local Wayland clipboard while
  the openterface-rs window is focused and types supported text into the target
  through the CH9329 keyboard path. The shortcut is configurable with
  `OPENTERFACE_PASTE_SHORTCUT`.
- Middle-click host paste can type the local primary selection or regular
  clipboard to the target when enabled with `OPENTERFACE_MIDDLE_CLICK_PASTE`;
  the default keeps middle-click forwarding to the target.

### Changed
- Removed historical implementation-comparison wording from docs and comments.
- Removed the obsolete implementation plan and its documentation links.
- The GUI window now opens **undecorated** by default (`OPENTERFACE_USE_LIBDECOR`
  defaults to `0`). Set `OPENTERFACE_USE_LIBDECOR=1` to draw a libdecor
  client-side title bar. See the fix below for why.

### Fixed
- GUI window no longer disappears (while the process keeps rendering) after a
  focus/visibility change such as returning to the window after a niri workspace
  switch. Root cause: winit's client-side decorations (CSD) commit the toplevel
  out of band from wgpu's surface presentation; on the configure that arrives
  when the window is re-shown the two race and the compositor unmaps the
  toplevel. Client-side decorations now default off (undecorated xdg-shell), so
  there is a single committer for the surface.
- Hardened the renderer against a genuinely stale Wayland surface: on
  `Outdated`/`Lost` from `get_current_texture` it now reconfigures the surface
  with the current window size and re-arms a redraw instead of silently dropping
  the error, and a refocus re-arms a redraw.

## [1.0.2] - 2026-06-08

### Added
- **Aspect-ratio-preserving video display.** The window now opens at a 16:9
  default size, and the renderer letterboxes/pillarboxes the captured frame
  (symmetric black bars) instead of stretching it to the window shape, so the
  target image keeps its proportions at any window size. Implemented as a pure
  `contain_scale` fit (`openterface-gui::fit`, unit-tested with no GPU) feeding a
  scale uniform to the blit shader.

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

openterface-rs is a native-Linux, Wayland-only Rust host application for the
Openterface Mini-KVM: video plus keyboard/mouse over one USB cable.
Hardware-validated in the work-ssd VM (device discovery, live capture, and
CH9329 injection moving the target cursor).

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

### Behavior notes
- Runtime failures exit **`1`**; usage errors exit `2`.
- `status` is detection-based rather than in-process connection state.
- No auto window when no device is found: `connect` errors instead (use `--dummy`
  for a deviceless window).
- `OPENTERFACE_USE_LIBDECOR=0` falls back to a bare xdg-shell window; decoration
  negotiation is handled by winit.

[Unreleased]: https://github.com/vicondoa/openterface-rs/compare/v1.0.2...HEAD
[1.0.2]: https://github.com/vicondoa/openterface-rs/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/vicondoa/openterface-rs/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/vicondoa/openterface-rs/releases/tag/v1.0.0
