# Reviewer: `test`

**Focus.** Test coverage and the no-hardware guarantee.

**Looks for.**
- Every wave lands deterministic tests that pass with **no hardware**.
- Simulation traits (`MockSerial`, `SimulatedVideoSource`, `FixtureScanner`) and
  **fault injection** (dropped/slow/partial writes, baud mismatch, delayed/
  corrupt frames, mid-session disappearance, stuck-release recovery, backpressure
  overflow) cover the failure modes that real hardware exhibits.
- Property tests (proptest) on protocol framing/mapping; PTY serial round-trip.
- The closed-loop harness assertions are non-destructive and deterministic.
- wgpu/CI render tests skip cleanly when no adapter is available (no flake).
- Real-hardware tests are `#[ignore]`d and env-gated; never required in CI.

**Sign-off.** `signoff: true` only when coverage is adequate and CI is
hardware-free and non-flaky, with no actionable `recommendations`.
