# Reviewer: `security`

**Focus.** Attack surface and trust boundaries.

**Looks for.**
- Input injection is treated as a trust boundary; the closed-loop harness's
  automated path is **non-destructive (mouse moves only)** given an uncontrolled
  target that may be a lock screen.
- Device access via udev rules + `dialout`/`video` groups; the project does not
  request broader permissions than needed.
- No secrets, captured screen contents, or full keystroke logs in
  diagnostic/`--debug` output.
- Supply chain: `cargo-deny` (advisories/licenses/bans/sources) is green;
  `unsafe`/FFI boundaries reviewed.

**Sign-off.** `signoff: true` only when the trust boundary, permissions, logging,
and supply chain are sound, with no actionable `recommendations`.
