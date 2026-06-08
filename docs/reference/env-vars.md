# Environment variables

Two groups: **runtime** variables that affect the running application, and
**harness/test** variables used only by the closed-loop tooling and test suite.

Invalid values fall back to the default (they are parsed leniently and never
abort startup), except where a range is enforced as noted.

## Runtime

| Variable | Default | Range / values | Effect |
|----------|--------:|----------------|--------|
| `RUST_LOG` | _(unset)_ | `error`/`warn`/`info`/`debug`/`trace`, or per-module | Log filter (via `tracing-subscriber`). |
| `OPENTERFACE_MOUSE_INTERVAL_MS` | `33` | accepted `5`–`1000` | Minimum interval between forwarded mouse-move commands (~30 Hz). A value outside the range (or unparseable) is ignored and the `33` default is used. **Load-bearing:** see below. |
| `OPENTERFACE_THROTTLE` | `1` | `0` disables | Idle MJPEG-decode throttling on/off. |
| `OPENTERFACE_IDLE_DECODE_MS` | `100` | ms | Minimum decode interval for a non-deterministic static stream. |
| `OPENTERFACE_INPUT_WAKE_MS` | `250` | ms | After input, decode at full rate for this long. |
| `OPENTERFACE_IDLE_WATCHDOG_MS` | `1000` | ms | Force a refresh at least this often (anti-freeze). |
| `OPENTERFACE_FULLSCREEN` | `0` | `0`/`1` (also `false`/`no`/`off` = off) | Start the window fullscreen. `0`/empty/`false` stay windowed. |
| `OPENTERFACE_REQUIRE_GPU` | _(unset)_ | set = require | Fail (not skip) if no wgpu adapter — used by the headless render test. |

> **`OPENTERFACE_MOUSE_INTERVAL_MS` is not an optimization.** Over USB/IP the
> CH9329 drains absolute-move commands at only ~30–40/sec. Forwarding faster
> overruns its buffer and delays key/button **releases**, so the target
> autorepeats or clicks miss. The default ~30 Hz is the verified safe rate.

## Harness / test

Used by [`tools/kvm-debug.sh`](../how-to/closed-loop-harness.md) and the test
suite only; they have no effect on the application.

| Variable | Default | Effect |
|----------|--------:|--------|
| `DRYRUN` | `0` | `1` = print framed CH9329 bytes instead of sending; no device access. |
| `KVM_PACE` | `0.004` | Seconds between CH9329 frames when sending (chip rate limit). |
| `KVM_CPU_MAX` | `25` | Diagnostic CPU%% warn threshold for the idle-throttle check. |
| `KVM_ALLOW_DESTRUCTIVE` | _(unset)_ | Required (with an explicit flag) to enable destructive verbs. See the harness guide. |
| `OPENTERFACE_HW_TESTS` | _(unset)_ | `1` = run the `#[ignore]`d real-device tests (never in CI). |

## Not yet supported

`OPENTERFACE_USE_LIBDECOR` (bare xdg-shell vs libdecor CSD toggle) from the C++
build is **not** wired in the Rust port yet — winit manages client-side
decorations automatically. Tracked for the parity gate.
