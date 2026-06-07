# Reviewer: `parity`

**Focus — feature-completeness gate.** Validates that openterface-rs supports
**every** capability of the C++ Openterface CLI before the project is "done".

**Checklist (must all be present and tested).**
- **Commands/flags:** `connect` (`--video`, `--serial`, `--no-video`,
  `--no-serial`, `--dummy`, `--debug`), `-v/--verbose`, `--version`, `scan`,
  `status`, `reset --serial`.
- **CH9329:** baud fallback; key press/release with modifiers; mouse move
  absolute & relative; buttons; scroll; `sendText`; Ctrl+Alt+Del; HID reset;
  factory reset; port enumeration; GET_INFO-silent tolerance.
- **Video:** V4L2 connect; start/stop; resolution/FPS/format (MJPG/YUYV); device/
  format/resolution enumeration; uvcvideo+MJPG auto-detect (skip virtio node).
- **Display:** GPU video window; window input capture; abs/rel modes; Esc exits
  relative; dummy mode.
- **Shipped behaviors:** ~30 Hz pacing + `OPENTERFACE_MOUSE_INTERVAL_MS`; idle
  MJPEG-decode throttle + `OPENTERFACE_THROTTLE` / `IDLE_DECODE_MS` /
  `INPUT_WAKE_MS` / `IDLE_WATCHDOG_MS`; niri/CSD windowing +
  `OPENTERFACE_USE_LIBDECOR` / `OPENTERFACE_FULLSCREEN`; resize off the input
  thread.
- **Integration:** runs under the work-ssd nixling VM; the closed-loop harness
  passes with no human.

**Sign-off.** `signoff: true` only when **all** of the above are present, tested,
and demonstrated under work-ssd, with no actionable `recommendations`. This
reviewer is mandatory at the final (W6) gate.
