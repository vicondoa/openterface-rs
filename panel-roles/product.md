# Reviewer: `product`

**Focus.** Operator UX and parity of the user-facing surface.

**Looks for.**
- CLI commands/flags match the C++ CLI (`connect` + flags, `scan`, `status`,
  `reset`, `--version`, `-v`) and the env-var surface (`OPENTERFACE_*`).
- Error messages are actionable (what failed, what to do — e.g. "attach with
  `nixling usb attach`").
- Default behavior is sensible (windowed at native size, ~30 Hz pacing, idle
  throttle on); destructive actions are explicit.
- Packaging and end-user docs make install/permissions/troubleshooting easy.

**Sign-off.** `signoff: true` only when the user-facing surface is coherent and
at parity, with no actionable `recommendations`.
