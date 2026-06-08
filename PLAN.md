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

**Current status:** `W6 — /etc/nixos integration + work-ssd validation` (Definition of Done) — next.
W5 Docs/packaging/harness CLOSED (101 hardware-free tests + 2 doctests; panel 10/10 across plan +
work review on GPT-5.5; merged PR #6). v1.0.0 prepared; tag cut in W6 after hardware validation.
Frontends` (CLI binary ∥ winit/wgpu display).
(71 hardware-free tests). W3 work-review panel gate in progress.
Integration` (pacing scheduler, session orchestration, vertical slice).
HID, decode, serial, video, discovery); 56 hardware-free tests + PTY test. W2
work-review panel gate in progress.
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

- [x] `W2.1` CH9329 command builders + golden/property tests.
- [x] `W2.2` HID usage tables + evdev->HID + modifier/sendText maps.
- [x] `W2.3` MJPEG (zune-jpeg) + YUYV->RGBA decode (real JPEG fixture).
- [x] `W2.4` serial baud-fallback + CH9329 response parse + serialport backend + PTY test.
- [x] `W2.5` fault-injecting video simulator + feature-gated v4l backend.
- [x] `W2.6` pure-sysfs discovery (skips virtio node).
- [x] **W2 panel gate** (rust, protocol, video, input, test, security) — unanimous 6/6 (W2fu2).

## W3 — Integration (×~2)

- [x] `W3.1` pacing scheduler (queue/coalesce/30Hz/release-priority, fake-clock; 11 tests).
- [x] `W3.2` session orchestration + shutdown/cancellation model (threads+channels).
- [x] `W3.3` end-to-end vertical slice (sim frame->decode + input->mock serial bytes).
- [x] **W3 panel gate** (rust, protocol, input, test, security) — unanimous 5/5 (W3fu3).

## W4 — Frontends (fan-out ×2)

- [x] `W4.1` CLI binary (scan/status/reset/connect + tracing + 10 assert_cmd tests; hardware feature).
- [x] `W4.2` winit/wgpu display + input capture + idle-decode throttle (headless render passes on lavapipe).
      tunables + idle-decode throttle + headless render test).
- [x] **W4 panel gate** (rust, input, video, product, test).

## W5 — Docs / packaging / harness (fan-out ×3 → serial cut)

- [x] `W5.1` Diataxis docs (`docs/` reference/how-to/explanation) + ARCHITECTURE
      + protocol reference + rustdoc (2 doctests; all-features `-D warnings` gate).
- [x] `W5.2` packaging: udev rules (uaccess + group fallback), checksum-verifying
      install.sh, release.yml (x86_64 + native-arm64 aarch64 + smoke), crates.io
      metadata, flake `packages.default` (wrapProgram for dlopen libs).
- [x] `W5.3` closed-loop harness finalize (`kvm-debug.sh`: DRYRUN frames,
      move/diff/rate/cpu non-destructive assertions, VID/PID-gated + fail-closed
      destructive verbs; hardware-free framing-drift test).
- [x] `W5.4` v1.0.0 prepared (workspace bump 0.0.0→1.0.0, CHANGELOG). Tag/publish
      DEFERRED to end of W6 (after real-hardware validation), per build-ci/docs.
- [x] **W5 panel gate** (docs, product, build-ci, security, test). Plan-review
      PASSED 5/5 (4 rounds); work-review PASSED 5/5 (after W5fu1).

## W6 — /etc/nixos integration + work-ssd validation (serial; Definition of Done)

- [x] `W6.1` Nix derivation (`rustPlatform.buildRustPackage`) — flake `packages.default`
      builds (`nix build .#default` → openterface-rs-1.0.0, runs, ships udev rules, wraps
      LD_LIBRARY_PATH for dlopen libs).
- [~] `W6.0` **parity feature-completeness audit (parity reviewer, GPT-5.5)** — found ~17
      gaps vs the C++ CLI. The wave gates only reviewed each wave's delta; this is the first
      full cross-cutting audit. **These are the gating work for the DoD `parity` sign-off.**

### W6 parity-completeness backlog (re-audited by parity reviewer on GPT-5.5)

  CLI:
  - [x] `-v/--verbose` prints `Verbose mode enabled`.
  - [x] `connect --debug` wired (logs input events; filter now includes openterface_gui).
  - [x] `connect --no-video` input-only mode via `Session::start_input_only`.
  - [ ] `status` parity — **documented deviation** (detection-based vs C++ in-process state).
  - [x] `scan` enumerates ALL video + serial nodes (`SysfsScanner::video_nodes/serial_nodes`).
  - [x] dummy-mode simulated-input messaging.
  - [ ] auto GUI-only mode when no device found — **documented deviation** (Rust errors; arguably
        better UX for a KVM tool than opening a blank window).
  CH9329 / serial:
  - [~] CH9329 mode 0x82 reconfigure — **SET_PARA_CFG (mode 0x82 / 115200) now implemented**
        (`ch9329::set_para_cfg` + `serial::reset_chip`) and wired into the factory reset (exact C++
        bytes, golden-tested). **DEFERRED:** the connect-time `GET_PARA_CFG` verify-and-reconfigure
        (read the chip's mode on connect and fix it) — `get_info`/baud-fallback connection works today.
  - [x] factory reset = RTS pulse (4s) + 1s + full `reset_chip` reconfigure to mode 0x82
        (`serial::factory_reset` + `set_rts` + `set_para_cfg`, matching C++ `factoryReset`).
  - [x] `reset_hid` shipped operation.
  - [x] `sendText` (`ch9329::text_to_reports` + `Session::send_text`).
  - [x] 4ms physical inter-command write gap (session writer; gap enforced *before* poll so a
        release arriving during the gap still jumps ahead of a pending move).
  - [ ] **DEFERRED** relative-mouse mode + long-press-Esc + pointer-lock — protocol/scheduler
        already support relative events; the GUI pointer-lock toggle needs live GUI+device
        iteration to implement reliably (can't be validated headlessly).
  V4L2:
  - [x] configure fallback chain 1080p MJPG → 720p MJPG → 720p YUYV + FPS (S_PARM, non-fatal).
  - [x] `supported_formats` populates `frame_rates`.
  - [x] discovery selects uvcvideo + advertised MJPG, skips virtio.
  Display:
  - [x] `OPENTERFACE_USE_LIBDECOR=0` bare xdg-shell, Wayland app-id, 640×480 min-size.
  - [x] resize deferred to redraw (off the input/event-dispatch path).

  **Status:** 15 fully closed + CH9329 mode-0x82 reconfigure now done (only the connect-time
  GET_PARA_CFG verify deferred). Remaining: relative-mouse mode (deferred, hardware-GUI), the
  connect-time mode verify (deferred). Documented deviations: detection-based `status`, no
  auto-GUI-only. The W6 panel (10 reviewers, GPT-5.5) drove these fixes: input 4ms-gap ordering,
  full factory-reset reconfigure, --debug keystroke redaction, scan partial-detection hints, docs.
  Hardware-validated on the host device: discovery picks the uvcvideo+MJPG node; closed-loop harness
  captures real frames
  (v4l2-ctl mmap) and CH9329 injection works (`move` + `diff`).

- [x] `W6.2` standalone closed-loop validation on the real device — harness `capture` writes a
      valid 1280×720 JPEG; `move` injects; `diff` runs (INCONCLUSIVE = correct for a non-static
      target screen). Device on host (work-ssd busids unbound); serial needs the udev rules
      (the /etc/nixos step, intentionally skipped) so injection was run via sudo for the test.
- [~] `W6.3` **panel `parity` re-audit** — 15/17 closed; not a full `done` sign-off (2 deferred +
      live-app closed-loop pending). NOTE: per user, **do not edit /etc/nixos**.
- [—] `W6.4` replace C++ in `work-ssd.nix` — **intentionally NOT done** (user: stop short of
      /etc/nixos). Template ready at `packaging/nixos/openterface-rs.nix`.
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
  `a73ff99`, `1084931`. PR #2.
- **2026-06-07 — W2 Core fan-out — CLOSED.** Panel 6/6 (input, test, security ✓ after `W2fu1`;
  protocol, video, rust ✓ after `W2fu2`), reviewers on GPT-5.5. Six modules: CH9329 builders +
  golden/property tests, HID tables (evdev→HID, sendText), MJPEG+YUYV decode (real fixture,
  range-correct colorimetry), serial baud-fallback + serialport backend + PTY test, fault-injecting
  video sim + v4l backend (timeout recovery), pure-sysfs discovery. 59 hardware-free tests. Panel
  caught: YUYV decode panics, wrong limited-range coefficients, v4l timeout wedge + panic, HID
  parity (evdev 43→0x32, +KEY_102ND). PR #3.
- **2026-06-07 — W3 Integration — CLOSED.** Panel 5/5 (protocol, security ✓ round 1; rust, test
  ✓ after `W3fu1`; input ✓ after `W3fu3`), reviewers on GPT-5.5. Pacing scheduler (30Hz coalesce
  + release-priority, fake-clock), session orchestration (threads + channel-disconnect shutdown,
  drains releases), end-to-end vertical slice. 75 hardware-free tests. Panel caught real
  input-ordering bugs: releases not jumping the batch at the session boundary, releases lost on
  shutdown, abs/rel position desync, and a flush-with-wrong-button-mask drag bug.
- **2026-06-07 — W4 Frontends — CLOSED.** Panel 5/5 (rust, test ✓ after `W4fu1`; input, product
  ✓ after `W4fu2`; video ✓ after `W4fu3`), reviewers on GPT-5.5. W4.1 CLI (scan/status/reset/
  connect + tracing + assert_cmd tests; `hardware` feature gates backends). W4.2 winit/wgpu
  display: pure coord/throttle/input_map modules (always tested) + winit/wgpu behind `display`
  feature; headless render verified on lavapipe. 98 hardware-free tests. Panel caught real bugs:
  modifier byte not cleared on focus loss / close, releases not flushed on window-close, idle-decode
  throttle wedging static streams (raw-changed tracking so wake re-decodes don't poison raw-dedup).
  PR #5.
- **2026-06-08 — W5 Docs / packaging / harness — CLOSED.** Plan-review 5/5 (docs, product, build-ci,
  security, test on GPT-5.5) after 4 refinement rounds; work-review 5/5 (test ✓ round 1; docs,
  product, security, build-ci ✓ after `W5fu1`). Diataxis docs tree, expanded ARCHITECTURE, README,
  CONTRIBUTING, SECURITY; least-privilege udev rules + checksum-verifying install.sh + release.yml
  (x86_64 + native-arm64) + flake `packages.default`; closed-loop harness (DRYRUN frames +
  non-destructive move/diff/rate/cpu + fail-closed destructive verbs) + hardware-free framing-drift
  test; v1.0.0 prep (version bump + CHANGELOG, tag deferred to W6). 101 hardware-free tests + 2
  doctests. Panel caught real bugs: OPENTERFACE_FULLSCREEN=0 wrongly enabled fullscreen, install.sh
  sudo-required for user prefix + unverified udev fallback download, SHA256SUMS ./-prefix breaking
  install.sh checksum match, several doc/code mismatches. PR #6.
