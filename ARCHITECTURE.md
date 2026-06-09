# Architecture

openterface-rs is a Cargo **workspace** split so that the device-agnostic core
carries no GUI or async-runtime weight, and everything that touches hardware is
a **trait** — which is what lets the entire pipeline be tested with no device.

```
openterface-rs/
├── crates/
│   ├── openterface-core/       # no GUI, no tokio, no hard device dependency
│   │   ├── protocol/           # CH9329 framing (ch9329) + HID tables (hid)
│   │   ├── serial/             # SerialTransport trait + baud constants
│   │   ├── video/              # VideoSource trait + Frame model
│   │   ├── decode/             # MJPEG / YUYV → RGBA
│   │   ├── discovery/          # DeviceScanner trait + DeviceInfo
│   │   ├── input/              # InputEvent → CH9329 forwarding glue
│   │   ├── pacing/             # paced CH9329 command scheduler
│   │   ├── session/            # orchestration of video + input + serial
│   │   ├── device.rs           # USB identity constants
│   │   ├── event.rs            # device-agnostic input-event model
│   │   └── error.rs            # crate error type
│   ├── openterface-cli/        # `openterface-rs` binary (clap)
│   ├── openterface-gui/        # winit + wgpu Wayland display
│   └── openterface-test-support/  # MockSerial, SimulatedVideoSource, FixtureScanner
```

## Trait seams (the test boundary)

Three traits isolate all hardware:

- **`serial::SerialTransport`** — raw byte read/write/baud over the CH9329 link.
  Real impl: `serialport`. Test impls: an in-memory mock and a Linux PTY.
- **`video::VideoSource`** — V4L2 capture producing `Frame`s. Real impl: `v4l`.
  Test impl: a synthetic frame source.
- **`discovery::DeviceScanner`** — Openterface enumeration. Real impl:
  udev/sysfs. Test impl: a fixture scanner over sample sysfs trees.

Because the session, protocol, decode, and input layers depend only on these
traits, the full pipeline runs deterministically against simulated devices.

## The two hardware halves

The Openterface presents two independent USB endpoints sharing only the cable:

| Half | Chip | Node | Carries |
|------|------|------|---------|
| Video | MS2109 HDMI capture | `/dev/videoN` (UVC/MJPEG) | the target's screen |
| Input | CH9329 USB-serial HID | `/dev/ttyACM0` (115200 8N1) | mouse + keyboard |

## Load-bearing behaviors

These are not optimizations — the device misbehaves without them:

- **CH9329 pacing (~30 Hz default).** Over USB/IP the chip drains absolute-move
  commands at ~30–40/sec. Mouse moves are paced (default 33 ms, tunable via
  `OPENTERFACE_MOUSE_INTERVAL_MS`) and **key/button releases jump ahead of the
  move backlog** so they never arrive late. Implemented as a queue-based
  scheduler in `pacing`.
- **Idle MJPEG-decode throttling.** Decoding every frame of a static screen
  wastes CPU; unchanged frames skip decode + GPU upload, with an input-activity
  wake and an anti-freeze watchdog (`OPENTERFACE_THROTTLE`,
  `OPENTERFACE_IDLE_DECODE_MS`, `OPENTERFACE_INPUT_WAKE_MS`,
  `OPENTERFACE_IDLE_WATCHDOG_MS`).
- **Discovery selects the Openterface capture by card name + USB identity**
  (card name containing `Openterface`, or the MS2109 VID/PID), which skips the
  virtio-media decoder adapter that also appears as `/dev/video*` in a VM. The
  closed-loop harness additionally checks for `uvcvideo` + `MJPG`.

## Concurrency model

The session uses **std threads + channels** (no async runtime): a capture
thread, a paced serial-writer thread, and the GUI event loop, coordinated by an
explicit shutdown/cancellation model (shutdown channel, bounded queues, join
policy, blocking-read timeouts) so stop never deadlocks. The
[threading model](docs/explanation/threading-model.md) explains the ownership
and shutdown rules in full.

## Frontends

Two frontends sit on top of the core:

- **`openterface-cli`** — the `openterface-rs` binary (clap). `scan`/`status`
  are pure-sysfs and work with no features; `connect`/`reset` need the
  `hardware` feature, which also pulls in the display frontend.
- **`openterface-gui`** — the native Wayland display (`winit` + `wgpu`). Pure
  modules (window→absolute coordinate mapping, the idle-decode throttle state
  machine, winit-key→HID mapping) are always compiled and tested; the actual
  winit/wgpu window is behind the `display` feature. The throttle implements the
  load-bearing idle-decode behavior above.

## Testing

Because the hardware seams are traits, the suite runs with no device and no
system libraries — see [testing & simulation](docs/explanation/testing-and-simulation.md)
and [`docs/`](docs/README.md) for the full documentation tree.
