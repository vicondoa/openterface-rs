# Testing & simulation

The headline guarantee: **`cargo test --workspace` passes with no hardware and
no system libraries.** This is achieved by putting every hardware interaction
behind a trait and testing against simulators, with the OS-backed
implementations gated behind off-by-default Cargo features.

## The three trait seams

| Trait | Real impl (feature) | Test impl |
|-------|---------------------|-----------|
| `serial::SerialTransport` | `serialport` (`serial-backend`) | `MockSerial` (scripted) + a Linux **PTY** round-trip |
| `video::VideoSource` | `v4l` (`video-backend`) | `SimulatedVideoSource` (synthetic MJPEG/YUYV) |
| `discovery::DeviceScanner` | udev/sysfs | `FixtureScanner` over sample sysfs trees |

Everything above these seams — protocol framing, decode, pacing, session
orchestration, discovery logic, coordinate mapping, input mapping, the
idle-decode throttle — is pure and always compiled and tested.

## Layers of tests

- **Unit / property** (`proptest`): CH9329 framing & checksums, abs/rel mouse
  construction, HID scancode/modifier mapping, baud-fallback state machine,
  VID/PID matching, YUYV→RGBA, MJPEG decode on golden JPEGs, with golden
  byte-vectors.
- **Simulation integration**: a full session against simulated devices — input
  events produce exact CH9329 byte sequences on the mock; pacing/throttle timing
  is asserted with a fake clock; frames flow through the decode pipeline.
- **PTY round-trip**: exercises the real `serialport` code path over a Linux
  pseudo-terminal — still no device.
- **CLI**: `assert_cmd` over the command surface, flags, exit codes,
  `--help`/`--version`.
- **Headless render**: a feature-gated wgpu render-to-buffer test on a software
  Vulkan adapter (Mesa lavapipe) so the GPU path is exercised in CI without a
  real GPU.
- **Harness framing drift**: a hardware-free test asserts the
  [closed-loop harness](../how-to/closed-loop-harness.md)'s `DRYRUN` frame output
  stays byte-identical to the Rust builders, so the bash tooling can't silently
  diverge from the protocol code.

## Simulators model *bad* hardware

The simulators are not just happy-path: they reproduce dropped/slow/partial
serial writes, baud mismatch, delayed/corrupt MJPEG frames, mid-session device
disappearance, stuck key/mouse release recovery, and backpressure overflow — so
the error-handling and shutdown paths are covered deterministically.

## Real-hardware tests

A tiny supplemental suite needs a real device. These are `#[ignore]`d and gated
behind `OPENTERFACE_HW_TESTS=1`; they **never** run in CI. The bulk of
real-device verification is the [closed-loop harness](../how-to/closed-loop-harness.md),
which runs headless in the work-ssd VM. Coverage is measured with
`cargo llvm-cov`.
