# Install

openterface-rs is **Linux-only** and runs natively on **Wayland** (no
XWayland, no Qt). Prebuilt binaries are provided for **x86_64** and **aarch64**
(e.g. Raspberry Pi running a 64-bit OS).

> **Note:** prebuilt release artifacts appear on the
> [Releases page](https://github.com/vicondoa/openterface-rs/releases) once a
> version is **tagged**. The first tag (`v1.0.0`) is cut after the real-hardware
> validation gate; until then, build [from source](build.md) or via Nix.

## Quickstart

```bash
# 1. Install the latest release (binary + udev rules).
curl -fsSL https://raw.githubusercontent.com/vicondoa/openterface-rs/main/packaging/install.sh -o install.sh
sh install.sh                      # inspect it first; it verifies its own download checksums

# 2. (headless/SSH only) join the device group, then re-login.
sudo usermod -aG openterface "$USER"

# 3. Plug in the Openterface, then verify.
openterface-rs --version
openterface-rs scan
openterface-rs status

# 4. Start a session.
openterface-rs connect
```

If you just plugged the device in and `scan` shows nothing, unplug/replug after
installing the udev rules (rules apply to devices added *after* they load), and
make sure your session/group membership is refreshed — see
[permissions & udev](permissions-udev.md).

## Option A — prebuilt binary via `install.sh` (recommended)

`packaging/install.sh` downloads the release tarball for your architecture,
**verifies its SHA-256 against the published `SHA256SUMS` before doing
anything**, then installs the binary (`/usr/local/bin`) and the udev rules.
Privilege is requested only for the final copy steps.

```bash
sh install.sh --version v1.0.0     # pin a version
sh install.sh --prefix "$HOME/.local" --no-udev
sh install.sh --uninstall
```

The release tarball contains: `openterface-rs`, `60-openterface.rules`,
`install.sh`, `README.md`, `LICENSE`, `CHANGELOG.md`. You can also download it
manually from the [Releases page](https://github.com/vicondoa/openterface-rs/releases)
and verify it yourself:

```bash
sha256sum -c SHA256SUMS
```

## Option B — `cargo install`

The display/hardware support is behind a feature flag, and it needs the system
libraries below at build time:

```bash
sudo apt-get install -y libudev-dev libv4l-dev libwayland-dev \
  libxkbcommon-dev libdecor-0-dev clang libclang-dev   # Debian/Ubuntu

cargo install --git https://github.com/vicondoa/openterface-rs \
  openterface-cli --features hardware --locked
```

Without `--features hardware` you get a binary that can `scan`/`status` but not
`connect`/`reset`. You must still install the [udev rules](permissions-udev.md)
separately.

## Option C — Nix

```bash
nix run github:vicondoa/openterface-rs            # run without installing
nix profile install github:vicondoa/openterface-rs
```

The Nix package wraps the binary so the Wayland/Vulkan libraries it `dlopen`s
are found at runtime, and ships the udev rules under
`$out/lib/udev/rules.d`.

## Runtime dependencies

A `connect` session needs, at runtime: a Wayland compositor, `libwayland-client`,
`libxkbcommon`, `libdecor`, a Vulkan loader + driver (`libvulkan` + e.g. Mesa),
`libGL`, plus `libudev` and `libv4l`. The prebuilt binary expects these present
on the system (they are standard on a Wayland desktop); the Nix package bundles
them via a wrapper.
