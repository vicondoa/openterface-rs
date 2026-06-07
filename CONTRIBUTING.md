# Contributing to openterface-rs

Thanks for your interest! This project targets **Linux + Wayland only**, is
**Qt-free**, and aims for a fast build and a test-suite that runs **without
hardware**.

## Getting started

```bash
git clone https://github.com/vicondoa/openterface-rs.git
cd openterface-rs
cargo build --workspace
cargo test --workspace
```

Or use the Nix dev shell (brings the toolchain and system libraries):

```bash
nix develop
```

## Before you open a PR

Run the same gates CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- New behavior needs **hardware-free tests** (use `openterface-test-support`).
- Hardware-only checks must be `#[ignore]`d and gated behind
  `OPENTERFACE_HW_TESTS=1` so they never run in CI.
- Keep `openterface-core` free of GUI and async-runtime dependencies.

## Workflow

`main` is protected. Develop on a branch, validate locally, open a PR, and let
CI run; changes land via squash-merge. See [`AGENTS.md`](AGENTS.md) for the full
process (panel review, versioning, commit conventions).

## Real-hardware testing

Some checks need an attached Openterface and a target. The closed-loop harness
(`tools/kvm-debug.sh`) injects raw input and captures frames so input/video can
be verified without a human watching a window.

> **Warning:** the injection verbs send real keystrokes/clicks to the attached
> computer. If the target is on a PIN/lock screen, injected keys can lock the
> account. Capture and look before you inject; automated checks use
> non-destructive mouse moves only.

## License

By contributing, you agree your contributions are licensed under
[Apache-2.0](LICENSE).
