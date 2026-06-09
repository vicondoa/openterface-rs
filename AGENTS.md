# Development process (AGENTS.md)

This document is the contract for how openterface-rs is built. It is adapted
from the nixling project's methodology for a Rust userland KVM tool.

## Workspace & toolchain

- Rust workspace; toolchain pinned in `rust-toolchain.toml` (1.95).
- Local validation before any gate:
  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace          # must pass with NO hardware
  ```

## Panel review (two gates per wave)

Large changes are organized into **waves**. A wave is the unit of review — the
nixling notion of a "phase". Each wave passes **two panel gates**, and the
integrator MUST NOT cross a gate without **unanimous sign-off** from the wave's
selected roster:

1. **Plan-review gate** (before dispatch) — the panel reviews the wave's task
   breakdown/approach. No implementation work begins until every selected
   reviewer returns `signoff: true`.
2. **Dispatch** the wave's tasks (in parallel where the dependency graph allows).
3. **Integration** — merge the work on the wave branch.
4. **Work-review gate** (after integration) — the panel reviews the integrated
   diff. Any findings spawn fix-subagents; land the fixes (commit-tagged),
   rerun tests, and run **another panel round**. The wave **closes only on
   unanimous sign-off** — green tests do not waive this gate.
5. **Advance** to the next wave's plan-review gate.

Each reviewer returns a JSON record; `signoff` is `true` **iff**
`recommendations` is empty:

```json
{ "engineer": "protocol", "signoff": true,
  "summary": "What was reviewed and the posture.", "recommendations": [] }
```

### Roster

The 10-engineer roster (role briefs in [`panel-roles/`](panel-roles)):

| Engineer | Focus |
|----------|-------|
| `rust` | API/trait shape, error propagation, `unsafe`/FFI boundaries, workspace dep direction, testability. |
| `protocol` | CH9329/MS2109/HID correctness: framing, checksums, mouse abs/rel scaling, scancodes, baud fallback. |
| `video` | V4L2 capture, MJPEG/YUYV negotiation, node selection, decode, idle-decode throttle, wgpu render/color. |
| `input` | Wayland input capture (niri/CSD), xkb→HID mapping, modifiers, focus/grab, rel-vs-abs, pacing/release-ordering. |
| `test` | No-hardware guarantee, simulation + fault injection, property tests, the closed-loop harness, regressions. |
| `security` | udev/permissions, input-injection trust boundary, harness lock-screen safety, supply chain, `unsafe`. |
| `product` | Operator UX, CLI/flag + env-var completeness, error messages, packaging, end-user docs. |
| `docs` | Diataxis docs, README/CHANGELOG/ARCHITECTURE, rustdoc, ADRs. |
| `build-ci` | Build speed, CI gates, cross-compile (x86_64 + aarch64), Nix derivation, release automation. |
| `parity` | **Feature-completeness gate**: every command/flag and every shipped behavior in the v1.0 scope. |

Each wave selects the relevant subset; the gate is unanimous over the selected
set (`N/N`). The final wave (W6 / Definition of Done) uses the **full 10/10**
including `parity`.

### Escape hatches

Narrow, as in nixling: trivial one-line/no-semantic fixes and documentation-only
changes may skip the gate unless load-bearing; a time-critical hotfix may skip
the pre-fix gate but MUST run a post-fix panel. When in doubt, run the panel.

## Versioning & changelog

- [Semantic Versioning](https://semver.org/) + [Keep a Changelog](https://keepachangelog.com/).
- Entries accumulate under `## [Unreleased]` during development.
- On release, rename to `## [X.Y.Z] - YYYY-MM-DD`, collapse to the standard
  groups (Added/Changed/Fixed/Deprecated/Removed/Security), and **strip all
  internal process markers** (wave/finding tags) from the released prose.

## Commit conventions

- Subject: short, imperative, area-prefixed — `protocol: frame absolute mouse`,
  `serial: add baud fallback`, `ci: gate clippy`.
- In-development commits on feature branches may carry a trailing wave tag:
  `Wn`, fix-rounds `Wnfu<M>`, with finding severity `C/H/M/L` + ordinal —
  e.g. `protocol: fix checksum wrap ( W2fu1 H3 )`. These markers are **not**
  allowed in shipped code, docs, or released CHANGELOG sections.

## Landing changes (PR workflow)

`main` is protected: changes land via pull requests, not direct pushes. Develop
on a branch, validate locally against the gates above, open a PR, let CI / panel
review run, then squash-merge. Self-merge is permitted (required approvals = 0).

## Test layout

| Location | Role |
|----------|------|
| crate `#[cfg(test)]` unit tests | Pure logic: framing/checksums, mapping, config. |
| `openterface-test-support` | `MockSerial`, `SimulatedVideoSource`, `FixtureScanner` (+ fault injection). |
| integration tests | Full session against simulated devices; PTY serial round-trip. |
| CLI tests | `assert_cmd`/`trycmd` for the command surface. |
| `#[ignore]` hardware suite | Real-device / closed-loop checks; gated by `OPENTERFACE_HW_TESTS=1`; never in CI. |

Every wave must land deterministic tests that pass with **no hardware**.
