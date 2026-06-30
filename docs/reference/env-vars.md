# Environment variables

Two groups: **runtime** variables that affect the running application, and
**harness/test** variables used only by the closed-loop tooling and test suite.

Invalid values fall back to the default (they are parsed leniently and never
abort startup), except where a range is enforced as noted.

## Runtime

| Variable | Default | Range / values | Effect |
|----------|--------:|----------------|--------|
| `RUST_LOG` | _(unset)_ | `error`/`warn`/`info`/`debug`/`trace`, or per-module | Log filter (via `tracing-subscriber`). |
| `OPENTERFACE_TITLE_PREFIX` | _(unset)_ | text | Prefix prepended to GUI window titles. Useful for labeling VM/proxy sessions in compositor title/status UI and client-side title bars. |
| `OPENTERFACE_MOUSE_INTERVAL_MS` | `33` | accepted `5`–`1000` | Minimum interval between forwarded mouse-move commands (~30 Hz). A value outside the range (or unparseable) is ignored and the `33` default is used. **Load-bearing:** see below. |
| `OPENTERFACE_THROTTLE` | `1` | `0` disables | Idle MJPEG-decode throttling on/off. |
| `OPENTERFACE_IDLE_DECODE_MS` | `100` | ms | Minimum decode interval for a non-deterministic static stream. |
| `OPENTERFACE_INPUT_WAKE_MS` | `250` | ms | After input, decode at full rate for this long. |
| `OPENTERFACE_IDLE_WATCHDOG_MS` | `1000` | ms | Force a refresh at least this often (anti-freeze). |
| `OPENTERFACE_FULLSCREEN` | `0` | `0`/`1` (also `false`/`no`/`off` = off) | Start the window fullscreen. `0`/empty/`false` stay windowed. |
| `OPENTERFACE_USE_LIBDECOR` | `0` | `0`/`1` (also `false`/`no`/`off`) | `0` (default) opens a bare xdg-shell window (no decorations), avoiding a CSD↔wgpu surface race that can make the window disappear on focus/visibility changes (e.g. returning after a niri workspace switch); `1` draws winit's client-side decorations (title bar). |
| `OPENTERFACE_WINDOW_MAX_SIZE` | capture size | `WIDTHxHEIGHT`, each dimension `1`–`32767` | Same as `connect --window-max-size`: caps the video/content area in physical pixels. The default is the negotiated capture size, usually `1920x1080`, so compositors do not configure the video above native resolution. |
| `OPENTERFACE_ENABLE_PASTE` | `1` | `0` disables | Enable focused GUI paste (`Ctrl+Shift+V` by default) from the local Wayland clipboard to the target. |
| `OPENTERFACE_PASTE_SHORTCUT` | `ctrl-shift-v` | modifier+`v` chord | Host-local focused paste shortcut. Accepted modifier tokens: `ctrl`, `alt`, `shift`, `super`; separators can be `+`, `-`, `_`, or spaces. Invalid values fall back to `ctrl-shift-v`. |
| `OPENTERFACE_MIDDLE_CLICK_PASTE` | `off` | `primary`/`clipboard`/`off` | Host-local middle-click paste source. `off` forwards middle-clicks to the target; `primary` types the local primary selection; `clipboard` types the regular clipboard. |
| `OPENTERFACE_PASTE_MAX_CHARS` | `4096` | accepted `1`–`65536` | Maximum normalized characters submitted per paste. Extra characters are truncated and reported; the cap is applied after CRLF/CR normalize to LF. |
| `OPENTERFACE_REQUIRE_GPU` | _(unset)_ | set = require | Fail (not skip) if no wgpu adapter — used by the headless render test. |

> **`OPENTERFACE_MOUSE_INTERVAL_MS` is not an optimization.** Over USB/IP the
> CH9329 drains absolute-move commands at only ~30–40/sec. Forwarding faster
> overruns its buffer and delays key/button **releases**, so the target
> autorepeats or clicks miss. The default ~30 Hz is the verified safe rate.

> **Paste throughput is paced.** Paste is sent as a keyboard-state stream through
> the same 4 ms CH9329 command gap as normal keyboard input. Repeated physical
> keys need extra release frames, but typical text is close to one report per
> character plus a final release; press Escape or unfocus/close the window to
> abort queued paste frames.

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
