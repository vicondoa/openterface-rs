# Security Policy

## Reporting a vulnerability

Please report security issues privately via GitHub's
[private vulnerability reporting](https://github.com/vicondoa/openterface-rs/security/advisories/new)
rather than opening a public issue. We aim to acknowledge reports promptly and
will coordinate a fix and disclosure timeline with you.

## Security-relevant surface

openterface-rs forwards keyboard and mouse input to an attached target computer
and captures its screen. Keep these properties in mind:

- **Input injection is a trust boundary.** The tool can type and click on the
  target. The closed-loop test harness deliberately restricts automated
  injection to **non-destructive mouse moves** because the target screen is
  uncontrolled and may be a lock screen (typing could lock an account).
- **Device permissions.** Access to `/dev/ttyACM*` and `/dev/video*` is granted
  via udev rules and the `dialout`/`video` groups; the project ships udev rules
  rather than requiring broad permissions.
- **No secrets in logs.** Diagnostic/`--debug` output must not include captured
  screen contents or injected keystrokes beyond what is necessary to debug.
- **Supply chain.** Dependencies are checked in CI with `cargo-deny`
  (advisories, licenses, bans). `unsafe`/FFI boundaries (serialport, v4l2,
  libudev, wgpu) are reviewed.

## Supported versions

During pre-1.0 development, only the latest `main` is supported.
