# Contributing to openterface-rs

Thanks for your interest! This project targets **Linux + Wayland only**, keeps a
lean native dependency set, and aims for a fast build and a test-suite that runs
**without hardware**.

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
cargo test --workspace --doc            # doctests (nextest skips these)
```

- New behavior needs **hardware-free tests** (use `openterface-test-support`).
- Hardware-only checks must be `#[ignore]`d and gated behind
  `OPENTERFACE_HW_TESTS=1` so they never run in CI.
- Keep `openterface-core` free of GUI and async-runtime dependencies.
- Heavy/OS-backed code goes behind an **off-by-default feature** (`hardware`,
  `display`, `serial-backend`, `video-backend`, `gpu-tests`) so the default
  build stays library-free. See [build](docs/how-to/build.md).
- Public APIs need rustdoc; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
  --all-features --no-deps` must be clean.
- Docs live under `docs/` ([Diataxis](https://diataxis.fr/)); user-facing
  changes (flags, env vars, behavior) update the relevant reference/how-to page.

## Workflow

`main` is protected. Develop on a branch, validate locally, open a PR, and let
CI run; changes land via squash-merge. See [`AGENTS.md`](AGENTS.md) for the full
process (panel review, versioning, commit conventions).

## Real-hardware testing

Some checks need an attached Openterface and a target. The closed-loop harness
([`tools/kvm-debug.sh`](tools/kvm-debug.sh), documented in
[docs/how-to/closed-loop-harness.md](docs/how-to/closed-loop-harness.md)) injects
raw input and captures frames so input/video can be verified without a human
watching a window.

> **Warning:** the destructive verbs send real keystrokes/clicks to the attached
> computer. If the target is on a PIN/lock screen, injected keys can lock the
> account. The automated path uses non-destructive **mouse moves only**;
> destructive verbs are manual-only and require an explicit opt-in.

## Releasing (maintainers)

Versioning follows [SemVer](https://semver.org/) and
[Keep a Changelog](https://keepachangelog.com/); see [`AGENTS.md`](AGENTS.md).

1. Move `CHANGELOG.md` `[Unreleased]` to a dated `[X.Y.Z]` section; strip
   internal process markers from shipped text.
2. Bump `version` in the workspace `Cargo.toml` (and the intra-workspace
   dependency versions) to `X.Y.Z`; commit.
3. Open a PR, let CI pass (including the release **dry-run** via the
   `workflow_dispatch` path of `release.yml`), and squash-merge.
4. Tag `vX.Y.Z` on `main`. The `Release` workflow builds the x86_64 + aarch64
   binaries, generates `SHA256SUMS`, smoke-tests the packaged binary, and
   publishes the GitHub Release.
5. Verify `install.sh` against the new release on a clean machine.

## License

By contributing, you agree your contributions are licensed under
[Apache-2.0](LICENSE).
