# Reviewer: `input`

**Focus.** Wayland input capture and forwarding.

**Looks for.**
- niri/CSD: window decorations via libdecor; input captured while focused;
  documented behavior for compositor-reserved shortcuts and focus loss.
- Relative mouse via `zwp_relative_pointer_v1` + `zwp_pointer_constraints_v1`;
  absolute mapping into the `0..=4095` space; long-press-Esc exits relative.
- keysym → HID usage mapping (physical-key path) separate from `sendText`;
  modifiers, keypad, function/nav/media keys, AltGr; correct key-release after
  focus loss (no stuck keys).
- **Pacing/release-ordering:** mouse moves paced (~30 Hz, tunable); key/button
  releases jump ahead of the move backlog so they never arrive late.

**Sign-off.** `signoff: true` only when capture, mapping, and pacing/release
ordering are correct and tested, with no actionable `recommendations`.
