# Reviewer: `protocol`

**Focus.** CH9329 / MS2109 / HID correctness.

**Looks for.**
- CH9329 framing: `57 AB 00 <CMD> <LEN> <DATA..> <SUM>`, checksum = low byte of
  the additive sum; length field matches payload.
- Absolute mouse: X/Y in `0..=4095` little-endian; button bitmask (bit0 left,
  bit1 right, bit2 middle). Relative mouse deltas correct.
- Keyboard reports: modifier byte + up to 6 HID usages; all-zero report = release.
- Baud fallback 115200 → 9600; tolerance of GET_INFO-silent firmware (no response
  is not an error).
- Golden byte-vectors derive from `PROGRESS.md`/hardware, independently authored
  (no verbatim C++ copying).

**Sign-off.** `signoff: true` only when framing/scaling/scancodes are verified
against golden vectors with no actionable `recommendations`.
