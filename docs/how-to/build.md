# Build from source

## Prerequisites

- A recent stable Rust toolchain (the pinned version is in
  [`rust-toolchain.toml`](../../rust-toolchain.toml)).
- For the **hardware** features only, system libraries (Debian/Ubuntu names):

  ```bash
  sudo apt-get install -y libudev-dev libv4l-dev libwayland-dev \
    libxkbcommon-dev libdecor-0-dev clang libclang-dev
  ```

## The no-hardware default

The default build pulls **no** system libraries and needs **no** device — this
is the project guarantee that makes the suite portable:

```bash
cargo build --workspace
cargo test  --workspace      # 90+ tests, no hardware, no system libs
```

Heavy/OS-backed code is behind off-by-default features so this stays true.

## Features

| Crate | Feature | Pulls in |
|-------|---------|----------|
| `openterface-core` | `serial-backend` | `serialport` + libudev |
| `openterface-core` | `video-backend` | `v4l` + libv4l + libclang (bindgen) |
| `openterface-cli` | `hardware` | core backends **and** the display frontend |
| `openterface-gui` | `display` | `winit` + `wgpu` (Wayland + Vulkan) |
| `openterface-gui` | `gpu-tests` | the headless render test |

Build the real binary:

```bash
cargo build -p openterface-cli --release --features hardware
```

## Nix dev shell

Brings the toolchain **and** all system libraries (and sets `LIBCLANG_PATH` /
`LD_LIBRARY_PATH`):

```bash
nix develop
cargo build -p openterface-cli --features hardware
```

Build the packaged binary directly:

```bash
nix build .#default
./result/bin/openterface-rs --version
```

## The gates CI runs

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace          # or: cargo test --workspace
cargo test --workspace --doc           # doctests (nextest skips these)
```

With the hardware features (needs the system libraries):

```bash
cargo clippy -p openterface-cli --features hardware --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
```

See [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for the full contribution
workflow.
