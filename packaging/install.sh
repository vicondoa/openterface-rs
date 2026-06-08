#!/usr/bin/env sh
# openterface-rs installer.
#
# Downloads a released `openterface-rs` binary for this architecture from GitHub
# Releases, VERIFIES its SHA-256 checksum BEFORE installing, then installs the
# binary and the udev rules. Privilege is requested only for the final copy
# steps; nothing downloaded is executed before verification.
#
# Usage:
#   install.sh [--version vX.Y.Z] [--prefix DIR] [--no-udev] [--uninstall]
#
#   --version vX.Y.Z   Install a specific tag (default: latest release).
#   --prefix DIR       Install prefix for the binary (default: /usr/local).
#                      The binary goes to $DIR/bin/openterface-rs.
#   --no-udev          Do not install/refresh udev rules.
#   --uninstall        Remove the binary and udev rules instead of installing.
#
# Environment:
#   DESTDIR            Staging root prepended to all install paths (packaging).
#   OPENTERFACE_REPO   Override the GitHub repo (default vicondoa/openterface-rs).
#
# Dependencies: a POSIX shell, curl, tar, and one of sha256sum/shasum.
set -eu

REPO="${OPENTERFACE_REPO:-vicondoa/openterface-rs}"
PREFIX="/usr/local"
DESTDIR="${DESTDIR:-}"
VERSION=""
DO_UDEV=1
UNINSTALL=0
UDEV_RULES_DIR="/etc/udev/rules.d"
UDEV_RULE_NAME="60-openterface.rules"
BIN_NAME="openterface-rs"

log()  { printf '%s\n' "openterface-rs: $*"; }
warn() { printf '%s\n' "openterface-rs: $*" >&2; }
die()  { printf '%s\n' "openterface-rs: error: $*" >&2; exit 1; }

# --- privilege helper: only used for the actual install/remove steps ---------
SUDO=""

# Is the nearest existing ancestor of $1 writable by the current user?
dir_writable() {
  d=$1
  while [ -n "$d" ] && [ "$d" != "/" ] && [ ! -e "$d" ]; do
    d=$(dirname "$d")
  done
  [ -w "$d" ]
}

need_root() {
  # When staging into DESTDIR we never touch real system paths, so no sudo.
  [ -n "$DESTDIR" ] && { SUDO=""; return 0; }
  # Only escalate if a destination is not already writable (e.g. a user prefix
  # like ~/.local needs no sudo; /usr/local and /etc/udev do).
  need=0
  dir_writable "$(dirname "$BIN_DEST")" || need=1
  if [ "$DO_UDEV" -eq 1 ]; then
    dir_writable "$(dirname "$RULE_DEST")" || need=1
  fi
  if [ "$need" -eq 0 ]; then
    SUDO=""
    return 0
  fi
  if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
  elif command -v sudo >/dev/null 2>&1; then
    SUDO="sudo"
  else
    die "need root (or sudo) to install to $PREFIX and $UDEV_RULES_DIR; re-run as root, use --prefix to a writable dir, or pass --no-udev"
  fi
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="${2:?--version needs a tag}"; shift 2 ;;
    --prefix)  PREFIX="${2:?--prefix needs a dir}"; shift 2 ;;
    --no-udev) DO_UDEV=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (try --help)" ;;
  esac
done

BIN_DEST="${DESTDIR}${PREFIX}/bin/${BIN_NAME}"
RULE_DEST="${DESTDIR}${UDEV_RULES_DIR}/${UDEV_RULE_NAME}"

# --- uninstall ---------------------------------------------------------------
if [ "$UNINSTALL" -eq 1 ]; then
  need_root
  log "removing $BIN_DEST"
  $SUDO rm -f "$BIN_DEST"
  if [ "$DO_UDEV" -eq 1 ]; then
    log "removing $RULE_DEST"
    $SUDO rm -f "$RULE_DEST"
    if [ -z "$DESTDIR" ] && command -v udevadm >/dev/null 2>&1; then
      { $SUDO udevadm control --reload && $SUDO udevadm trigger; } || true
    fi
  fi
  log "uninstalled."
  exit 0
fi

# --- detect architecture / target triple -------------------------------------
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64)  TARGET="x86_64-unknown-linux-gnu" ;;
  aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
  *) die "unsupported architecture: $arch (prebuilt binaries: x86_64, aarch64)" ;;
esac
[ "$(uname -s)" = "Linux" ] || die "openterface-rs is Linux-only (saw $(uname -s))"

# --- tooling -----------------------------------------------------------------
command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar  >/dev/null 2>&1 || die "tar is required"
if command -v sha256sum >/dev/null 2>&1; then
  SHACMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  SHACMD="shasum -a 256"
else
  die "need sha256sum or shasum to verify the download"
fi

# --- resolve version ---------------------------------------------------------
api="https://api.github.com/repos/${REPO}/releases"
if [ -z "$VERSION" ]; then
  log "resolving latest release of $REPO ..."
  VERSION="$(curl -fsSL "${api}/latest" \
    | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name" *: *"([^"]+)".*/\1/')"
  [ -n "$VERSION" ] || die "could not determine the latest release tag"
fi
log "installing $REPO $VERSION ($TARGET)"

TARBALL="${BIN_NAME}-${VERSION}-${TARGET}.tar.gz"
base="https://github.com/${REPO}/releases/download/${VERSION}"

# --- download to a private temp dir (as the current, unprivileged user) ------
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM
log "downloading $TARBALL ..."
curl -fsSL "${base}/${TARBALL}"     -o "$tmp/$TARBALL"
curl -fsSL "${base}/SHA256SUMS"     -o "$tmp/SHA256SUMS"

# --- verify checksum BEFORE touching anything --------------------------------
log "verifying SHA-256 checksum ..."
expected="$(grep " ${TARBALL}\$" "$tmp/SHA256SUMS" | awk '{print $1}')"
[ -n "$expected" ] || die "no checksum for $TARBALL in SHA256SUMS"
actual="$(cd "$tmp" && $SHACMD "$TARBALL" | awk '{print $1}')"
[ "$expected" = "$actual" ] || die "checksum MISMATCH for $TARBALL (expected $expected, got $actual) — refusing to install"
log "checksum OK ($actual)"

# --- extract (still unprivileged) --------------------------------------------
tar -C "$tmp" -xzf "$tmp/$TARBALL"
SRC_BIN="$tmp/$BIN_NAME"
[ -f "$SRC_BIN" ] || SRC_BIN="$(find "$tmp" -type f -name "$BIN_NAME" | head -1)"
[ -f "$SRC_BIN" ] || die "extracted archive did not contain a '$BIN_NAME' binary"

# --- install (privileged only from here) -------------------------------------
need_root
log "installing binary -> $BIN_DEST"
$SUDO install -D -m 0755 "$SRC_BIN" "$BIN_DEST"

if [ "$DO_UDEV" -eq 1 ]; then
  # Only install the rules file that came from the checksum-verified tarball.
  # We deliberately do NOT fall back to downloading the rules separately: the
  # release publishes SHA256SUMS for the tarball only, so a standalone download
  # would be installed into /etc/udev/rules.d unverified.
  RULE_SRC="$(find "$tmp" -type f -name "$UDEV_RULE_NAME" | head -1 || true)"
  if [ -n "$RULE_SRC" ]; then
    log "installing udev rules -> $RULE_DEST"
    # The fallback group must exist before the rule references it.
    if [ -z "$DESTDIR" ] && command -v groupadd >/dev/null 2>&1; then
      getent group openterface >/dev/null 2>&1 || $SUDO groupadd --system openterface || true
    fi
    $SUDO install -D -m 0644 "$RULE_SRC" "$RULE_DEST"
    if [ -z "$DESTDIR" ] && command -v udevadm >/dev/null 2>&1; then
      log "reloading udev ..."
      { $SUDO udevadm control --reload && $SUDO udevadm trigger; } || true
    fi
    log "note: for headless/SSH use, add yourself to the group: sudo usermod -aG openterface \"\$USER\" (then re-login)"
  else
    warn "udev rules not found in the verified archive; skipping (use --no-udev to silence)"
  fi
fi

log "done. Verify with: $BIN_NAME --version && $BIN_NAME scan"
