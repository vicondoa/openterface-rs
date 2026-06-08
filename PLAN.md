# openterface-rs — Implementation Progress

> **Purpose.** This is the resumable, in-repo execution checklist for building
> openterface-rs. It mirrors the session plan and is updated **at every wave
> boundary** so work can be picked up if a session is interrupted. Check items
> off as they land. The authoritative *design* lives in the project docs
> (`ARCHITECTURE.md`, `AGENTS.md`); this file is the *progress tracker*.

## How to resume

1. Read the **Current status** line below to see the active wave.
2. Open that wave's section; the first unchecked `[ ]` item is the next step.
3. Each wave is bracketed by two **panel gates** (plan-review before dispatch,
   work-review after integration) — both must reach unanimous sign-off before
   the wave is marked done. See `AGENTS.md` → *Panel review*.
4. Validate locally before crossing a gate: `cargo fmt --check`,
   `cargo clippy --all-targets -- -D warnings`, `cargo test` (and `nextest`
   once wired).

**Current status:** `W1 — Spikes + behavioral spec` **complete** (panel 6/6).
Next: `W2 — Core fan-out` (protocol, HID, decode, serial, video, discovery).

---

## Process invariants (apply to every wave)

- [x] Toolchain available (Rust 1.95 via nix profile: cargo/rustc/clippy/rustfmt).
- [x] `main` is protected after initial setup: branches → PR → squash-merge,
      CI gating, self-merge allowed (required approvals = 0), direct pushes blocked.
- [ ] Each wave: plan-review panel gate → dispatch → integrate → work-review
      panel gate → advance. Unanimous `N/N` sign-off; green tests do **not**
      waive the gate.
- [ ] Commit tags: `Wn` / `Wnfu<M>` + severity `C/H/M/L`; markers stripped from
      released artifacts (CHANGELOG/docs/CLI text).
- [ ] Every wave lands: deterministic tests that pass with **no hardware**.

---

## W0 — Foundation (serial; 1 integrator)

*Goal: a compiling workspace with the interface contracts everything depends on,
CI, license/docs stubs, and the public repo created + protected.*

### Scaffold
- [x] Directory tree + `git init`.
- [x] Root `Cargo.toml` workspace (members, `workspace.package`,
      `workspace.dependencies`, `workspace.lints`, profiles).
- [x] `.gitignore`, `rustfmt.toml`, `rust-toolchain.toml`.

### Interface contracts (`openterface-core`)
- [x] `Cargo.toml`.
- [x] `lib.rs` (module map + re-exports).
- [x] `error.rs` (`Error` / `Result`).
- [x] `device.rs` (USB identity constants + `is_serial_device`/`is_video_device` + tests).
- [x] `event.rs` (`InputEvent`, `Modifiers`, `MouseButton`, `AbsPosition`, release/move helpers + tests).
- [x] `serial/mod.rs` (`SerialTransport` trait + baud constants).
- [x] `video/mod.rs` (`VideoSource` trait, `Frame`, `PixelFormat`, `CaptureConfig`, `FormatDesc`).
- [x] `discovery/mod.rs` (`DeviceScanner` trait + `DeviceInfo`).
- [x] `decode/mod.rs` (`RgbaImage` + stub `decode_frame` for W2.3).
- [x] `protocol/mod.rs` + `protocol/ch9329.rs` (frame prefix + checksum + test) + `protocol/hid.rs` (stub).
- [x] `input/mod.rs` (stub for W3).
- [x] `pacing/mod.rs` (`PacingConfig`, `DEFAULT_MOUSE_INTERVAL` 33ms, `from_env` + test).
- [x] `session/mod.rs` (stub for W3.2).

### Other crates (skeletons)
- [x] `openterface-cli` (`Cargo.toml` + `main.rs` printing version).
- [x] `openterface-gui` (`Cargo.toml` + `lib.rs` stub; winit/wgpu deferred to W4.2).
- [x] `openterface-test-support` (`MockSerial`, `SimulatedVideoSource`, `FixtureScanner`).

### Tooling, docs, process
- [x] CI workflow `.github/workflows/ci.yml` (fmt, clippy `-D warnings`, build, nextest, cargo-deny/audit).
- [x] `deny.toml` (cargo-deny config).
- [x] `flake.nix` dev shell (toolchain + system libs).
- [x] `LICENSE` (Apache-2.0).
- [x] Doc stubs: `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`.
- [x] `AGENTS.md` (panel review, versioning, commit conventions, test layout).
- [x] `panel-roles/*.md` (rust, protocol, video, input, test, security, product, docs, build-ci, parity).
- [x] `tools/kvm-debug.sh` harness skeleton.

### Green + publish
- [x] `cargo fmt --check` clean.
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo test` green (no hardware) — 12 tests.
- [x] Initial commit.
- [x] Create public repo `github.com/vicondoa/openterface-rs` + push.
- [x] Enable branch protection (approvals = 0, block direct pushes).
- [x] **W0 panel gate** (rust, build-ci, security, docs, test) — unanimous 5/5.

---

## W1 — Spikes + behavioral spec (fan-out ×5)

- [x] `W1.1` Wayland-input go/no-go on **niri** — **GO with winit** (niri advertises
      relative-pointer + pointer-constraints; winit smoke test: window + Locked/Confined
      grab OK; CSD via winit). `docs/explanation/wayland-input-spike.md`.
- [x] `W1.2` V4L2 capability spike — approach fixed (`v4l` crate, uvcvideo+MJPG select,
      synthetic fixtures; real-device fixtures deferred to work-ssd W6). `docs/explanation/v4l2-spike.md`.
- [x] `W1.3` wgpu/CI headless-render spike — feature-gated software-adapter test that
      skips when no adapter; render design fixed. `docs/explanation/wgpu-spike.md`.
- [x] `W1.4` clap skeleton + `--help`/`--version`/exit-code snapshots (`docs/reference/cli-help.txt`, 7 tests).
- [x] `W1.5` behavioral-compat spec from the C++ CLI — `docs/reference/cpp-cli-behavior.md` (422 lines, file:line cited).
- [x] **W1 panel gate** (rust, input, video, product, test, parity) — unanimous 6/6 (W1fu1).

## W2 — Core fan-out (fan-out ×6)

- [ ] `W2.1` `protocol/ch9329` encode/decode + checksums + golden vectors.
- [ ] `W2.2` `protocol/hid` usage tables + keysym→HID (physical vs `sendText` split).
- [ ] `W2.3` `decode` MJPEG (zune-jpeg) + YUYV→RGBA (← W1.2 fixtures).
- [ ] `W2.4` `serial` transport (serialport + `MockSerial` + fault injection + PTY test + baud fallback).
- [ ] `W2.5` `video` source (v4l impl + `SimulatedVideoSource` + fault injection).
- [ ] `W2.6` `discovery` (udev/sysfs + `FixtureScanner` + `scan`/`status`; uvcvideo+MJPG selection).
- [ ] **W2 panel gate** (rust, protocol, video, input, test, security).

## W3 — Integration (×~2)

- [ ] `W3.1` pacing scheduler (queue/backpressure/coalescing, fake-clock, 30 Hz, release-jump).
- [ ] `W3.2` session orchestration + shutdown/cancellation model + abs/rel + special keys.
- [ ] `W3.3` end-to-end vertical slice (sim frame→decode→texture→window; input→HID→mock serial).
- [ ] **W3 panel gate** (rust, protocol, input, test, security).

## W4 — Frontends (fan-out ×2)

- [ ] `W4.1` CLI binary (connect/scan/status/reset + dummy + tracing + assert_cmd/trycmd).
- [ ] `W4.2` display (winit/wgpu + window input capture + niri CSD + all `OPENTERFACE_*`
      tunables + idle-decode throttle + headless render test).
- [ ] **W4 panel gate** (rust, input, video, product, test).

## W5 — Docs / packaging / harness (fan-out ×3 → serial cut)

- [ ] `W5.1` Diataxis docs + ARCHITECTURE + protocol reference + rustdoc.
- [ ] `W5.2` packaging: udev rules, install.sh, release workflow (x86_64 + aarch64), crates.io, flake.
- [ ] `W5.3` closed-loop harness finalize (`kvm-debug.sh` + `debug` subcommand + non-destructive assertions).
- [ ] `W5.4` cut **v1.0.0** (serial).
- [ ] **W5 panel gate** (docs, product, build-ci, security, test).

## W6 — /etc/nixos integration + work-ssd validation (serial; Definition of Done)

- [ ] `W6.1` Nix derivation (`rustPlatform.buildRustPackage`).
- [ ] `W6.2` standalone validation in work-ssd via closed-loop harness (`nixling usb attach`).
- [ ] `W6.3` **panel `parity` feature-complete sign-off** (full 10/10; closed-loop green).
- [ ] `W6.4` **replace** the C++ derivation in `work-ssd.nix` + `nixling` switch + re-validate.
- [ ] **W6 panel gate** (full roster incl. `parity`) = **Definition of Done**.

---

## Wave completion log

_Append a one-line entry when a wave closes (date, wave, panel result, notes)._

- **2026-06-07 — W0 Foundation — CLOSED.** Panel 5/5 (rust, test, security, build-ci, docs);
  `W0fu1` fixed CI toolchain pin + PLAN sync + rustdoc link, re-confirmed on GPT-5.5.
  Public repo created, main protected (PR + CI, 0 approvals, no direct push), CI green.
  12 hardware-free tests. Commits `07290a0`, `089cbe0`.
- **2026-06-07 — W1 Spikes + behavioral spec — CLOSED.** Panel 6/6 (rust, input ✓ round 1;
  parity, video, test, product ✓ after `W1fu1`), reviewers on GPT-5.5. W1.1 **GO with winit**
  (niri advertises relative-pointer + pointer-constraints; winit smoke test passed live).
  W1.4 clap surface at C++ parity (now 10 tests). W1.5 behavioral spec (`cpp-cli-behavior.md`).
  W1.2/W1.3 capture+render designs fixed; Frame gained stride/colorimetry. Commits `a3451f8`,
  `a73ff99`, `1084931`.
