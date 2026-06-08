# Security Policy

## Reporting a vulnerability

Please report security issues privately via GitHub's
[private vulnerability reporting](https://github.com/vicondoa/openterface-rs/security/advisories/new)
rather than opening a public issue. We aim to acknowledge reports promptly and
will coordinate a fix and disclosure timeline with you.

## Device-access model

Access to the Openterface nodes (`/dev/ttyACM*` for the CH9329 input bridge and
`/dev/video*` for the MS2109 capture) is granted with **least privilege** by the
shipped udev rules
([`packaging/udev/60-openterface.rules`](packaging/udev/60-openterface.rules)):

- **`uaccess` (preferred).** The rules tag the nodes with `uaccess`, so the user
  logged in on the local seat receives an ACL from `systemd-logind`. No standing
  group membership is required on a desktop session.
- **Dedicated-group fallback.** For headless/SSH/VM use (no active seat), the
  nodes are owned by a dedicated **`openterface`** group with mode `0660`. This
  is intentionally **not** `MODE="0666"` (world access) and **not** the broad
  `dialout`/`video` groups, which would expose unrelated serial/video devices.

See [permissions & udev](docs/how-to/permissions-udev.md) for setup.

## Security-relevant surface

openterface-rs forwards keyboard and mouse input to an attached target computer
and captures its screen. Keep these properties in mind:

- **Input injection is a trust boundary.** The tool can type and click on the
  target. The closed-loop test harness deliberately restricts **automated**
  injection to **non-destructive mouse moves** because the target screen is
  uncontrolled and may be a lock screen (typing could lock an account).
  Destructive harness verbs (type/click/key) are **manual-only**: they require
  both `KVM_ALLOW_DESTRUCTIVE=1` and an explicit acknowledgement flag, print a
  loud warning, and fail closed otherwise.
- **Injection targets are VID/PID-verified.** Auto-detection only drives a serial
  node confirmed to be a CH9329 (`1A86:7523`/`1A86:FE0C`); it will not inject
  into an arbitrary `/dev/ttyACM*`/`/dev/ttyUSB*`.
- **Device permissions.** Granted via the least-privilege udev model above.
- **No secrets in logs.** Diagnostic/`--debug` output must not include captured
  screen contents or injected keystrokes/typed strings. Captured frames may
  contain sensitive content and are never uploaded by CI.
- **Supply chain.** Dependencies are checked in CI with `cargo-deny`
  (advisories, licenses, bans, sources). `unsafe`/FFI boundaries (serialport,
  v4l2, libudev, wgpu) are reviewed. Release artifacts ship a `SHA256SUMS` and
  `install.sh` verifies downloads before installing.

## Supported versions

During pre-1.0 development, only the latest `main` is supported.
