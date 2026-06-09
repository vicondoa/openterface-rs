# openterface-rs documentation

Documentation is organized along the four [Diataxis](https://diataxis.fr/)
directions.

## Reference (information-oriented)

- [CLI reference](reference/cli.md) — every command, flag, and exit code.
- [Environment variables](reference/env-vars.md) — runtime tunables and
  harness/test variables, with defaults and ranges.
- [CH9329 protocol](reference/protocol.md) — the serial command framing this
  implementation uses.
- [Device IDs & node selection](reference/device-ids.md) — USB VID/PID and how
  the right `/dev` nodes are chosen.

## How-to guides (task-oriented)

- [Install](how-to/install.md) — binaries, `cargo install`, Nix.
- [Permissions & udev](how-to/permissions-udev.md) — non-root device access.
- [Build from source](how-to/build.md) — workspace, features, dev shell.
- [Troubleshooting](how-to/troubleshooting.md) — common failures and fixes.
- [Closed-loop harness](how-to/closed-loop-harness.md) — verify a real device
  with no human.

## Explanation (understanding-oriented)

- [Architecture](../ARCHITECTURE.md) — workspace layout and the trait seams.
- [Threading model](explanation/threading-model.md) — the no-async concurrency
  and shutdown design.
- [Testing & simulation](explanation/testing-and-simulation.md) — how the suite
  runs with no hardware.
- Spikes: [Wayland input](explanation/wayland-input-spike.md),
  [V4L2](explanation/v4l2-spike.md), [wgpu](explanation/wgpu-spike.md).

## Status

v1.0 targets the core KVM workflow: video plus keyboard/mouse over one USB
cable. Real-hardware validation runs in the work-ssd VM via the closed-loop
harness. See [`../PLAN.md`](../PLAN.md) for the roadmap.
