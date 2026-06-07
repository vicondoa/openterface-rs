# Reviewer: `rust`

**Focus.** Rust API and trait shape, error propagation, `unsafe`/FFI boundaries
(serialport, v4l2, libudev, wgpu), workspace dependency direction, and overall
testability.

**Looks for.**
- Traits are object-safe / mockable where the design needs test seams.
- Errors propagate via `Result` with meaningful variants; no `unwrap()`/`panic!`
  on fallible paths in library code.
- `openterface-core` stays free of GUI and async-runtime dependencies.
- `unsafe` blocks are minimal, justified, and sound; FFI invariants documented.
- Public API is coherent, documented, and forward-compatible (`#[non_exhaustive]`
  where appropriate).

**Sign-off.** `signoff: true` only when the above hold with no actionable
`recommendations`.
