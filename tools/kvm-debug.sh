#!/usr/bin/env bash
#
# kvm-debug — closed-loop diagnostics for an Openterface mini-KVM.
#
# The Openterface exposes two USB endpoints: an MS2109 HDMI capture device
# (UVC / MJPEG video) and a CH9329 USB-serial HID bridge (mouse + keyboard).
# This tool talks to both DIRECTLY — it injects raw CH9329 input on the serial
# device and grabs frames from the capture device — so input and video can be
# verified WITHOUT a human watching the GUI window. Pair `capture` with the
# input verbs to form a closed loop: inject, capture, look at the JPEG.
#
# Dependency-light (bash + coreutils + v4l-utils + ffmpeg) so it runs inside a
# headless VM over SSH.
#
# SAFETY: the target screen is uncontrolled and may be a lock screen. AUTOMATED
# verification uses NON-DESTRUCTIVE perturbations only (mouse moves). The
# destructive verbs (type/click/key) are manual and must be opted into
# explicitly. Set DRYRUN=1 to print framed CH9329 bytes instead of sending.
#
# Status: SKELETON. Device detection + CH9329 framing are wired here; the
# automated assertions (frame-liveness, mouse-move pixel-diff, 30 Hz-settle
# regression, idle-CPU throttle) are finalized in wave W5.3.

set -euo pipefail

PACE="${KVM_PACE:-0.004}"      # seconds between CH9329 frames (chip rate limit)
DRYRUN="${DRYRUN:-0}"

die() { echo "kvm-debug: $*" >&2; exit 1; }

# ---- device detection (mirrors openterface-rs auto-detect) ------------------

detect_video() {
  local v drv
  for v in /dev/video*; do
    [ -e "$v" ] || continue
    drv=$(v4l2-ctl --device="$v" --info 2>/dev/null | grep -i "driver name" || true)
    case "$drv" in *uvcvideo*) ;; *) continue ;; esac
    if v4l2-ctl --device="$v" --list-formats 2>/dev/null | grep -qi mjpg; then
      printf '%s\n' "$v"; return 0
    fi
  done
  return 1
}

detect_serial() {
  local s
  for s in /dev/ttyACM* /dev/ttyUSB*; do
    [ -e "$s" ] && { printf '%s\n' "$s"; return 0; }
  done
  return 1
}

# ---- CH9329 framing ---------------------------------------------------------
# Every command is `57 AB 00 <CMD> <LEN> <DATA..> <SUM>`, SUM = low byte of the
# additive sum of all preceding bytes. (Matches openterface_core::protocol::ch9329.)

frame_hex() {
  local cmd=$1; shift
  local data=( "$@" )
  local frame=( 0x57 0xAB 0x00 "$cmd" "${#data[@]}" "${data[@]}" )
  local sum=0 n out=""
  for n in "${frame[@]}"; do sum=$(( (sum + n) & 0xFF )); done
  frame+=( "$sum" )
  for n in "${frame[@]}"; do out+=$(printf '%02X ' "$(( n ))"); done
  printf '%s\n' "${out% }"
}

usage() {
  cat <<'EOF'
kvm-debug <command>

  devices                 Print the detected capture + serial nodes.
  capture <out.jpg>       Grab one live frame of the target.
  frame-bytes             (DRYRUN) print a sample CH9329 absolute-move frame.

  (W5.3) move/diff/rate/flood/cpu — automated non-destructive assertions.

Env: DRYRUN=1 (print frames, don't send), KVM_PACE (inter-frame seconds).
EOF
}

main() {
  local cmd="${1:-}"; shift || true
  case "$cmd" in
    devices)
      echo "video:  $(detect_video || echo '(none)')"
      echo "serial: $(detect_serial || echo '(none)')"
      ;;
    capture)
      local out="${1:?usage: kvm-debug capture <out.jpg>}"
      local v; v=$(detect_video) || die "no MJPG capture device found"
      ffmpeg -hide_banner -loglevel error -f v4l2 -input_format mjpeg \
        -i "$v" -frames:v 1 -y "$out"
      echo "wrote $out"
      ;;
    frame-bytes)
      # Absolute mouse to screen centre (2048,2048), no buttons: CMD=04.
      frame_hex 0x04 0x02 0x00 0x08 0x00 0x08 0x00
      ;;
    ""|-h|--help|help)
      usage
      ;;
    *)
      die "unknown command '$cmd' (try: kvm-debug help)"
      ;;
  esac
}

main "$@"
