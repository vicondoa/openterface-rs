#!/usr/bin/env bash
#
# kvm-debug — closed-loop diagnostics for an Openterface mini-KVM.
#
# The Openterface exposes two USB endpoints: an MS2109 HDMI capture device
# (UVC / MJPEG video) and a CH9329 USB-serial HID bridge (mouse + keyboard).
# This tool talks to both DIRECTLY — it injects raw CH9329 input on the serial
# device and grabs frames from the capture device — so input and video can be
# verified WITHOUT a human watching the GUI window.
#
# Dependency-light for headless SSH use: bash + coreutils + awk + v4l-utils +
# ffmpeg. Pixel metrics use ffmpeg->raw gray->od->awk streams so captures stay in
# memory and are not written anywhere unless the user explicitly asks.

set -euo pipefail

PACE="${KVM_PACE:-0.004}"      # seconds between CH9329 frames (chip rate limit)
DRYRUN="${DRYRUN:-0}"
CPU_MAX="${KVM_CPU_MAX:-25}"

WIDTH=1280
HEIGHT=720
FRAME_PIXELS=$(( WIDTH * HEIGHT ))
STATIC_THRESHOLD=2000
LUMA_THRESHOLD=24

ABS_MIN=0
ABS_MAX=4095
ABS_CENTRE=2048
MOVE_START_X=$ABS_MIN
MOVE_START_Y=$ABS_MIN
MOVE_END_X=$ABS_MAX
MOVE_END_Y=$ABS_MAX
CLICK_X=$ABS_CENTRE
CLICK_Y=$ABS_CENTRE

SERIAL_OVERRIDE=""
VIDEO_OVERRIDE=""
DESTRUCTIVE_OK_FLAG=0

is_dryrun() { [ "$DRYRUN" = "1" ]; }

die() { echo "kvm-debug: $*" >&2; exit 1; }

warn() { echo "kvm-debug: $*" >&2; }

usage() {
  cat <<'EOF_USAGE'
kvm-debug [--serial DEV] [--video DEV] <command> [args]

Device / setup:
  preflight              Verify required tools: ffmpeg, v4l2-ctl, awk, od, wc.
  devices                Print detected UVC/MJPEG video and verified CH9329 serial nodes.
  capture <out.jpg>      Grab one 1280x720 MJPEG frame to the explicit output path.

Dry-run / framing:
  frames                 With DRYRUN=1, print canonical labelled CH9329 frames:
                         MOVE_ABS, CLICK_PRESS, CLICK_RELEASE, TYPE_A_PRESS,
                         TYPE_A_RELEASE, KEY_A_PRESS, KEY_A_RELEASE,
                         MOVE_REL, ABS_CENTRE.
  frame-bytes            With DRYRUN=1, print the canonical abs-centre frame only.

Non-destructive automated verbs (mouse moves only):
  move                   Send corner-to-corner absolute mouse moves; ends at bottom-right.
  diff                   Closed-loop pixel assertion. Exit 0 PASS, 1 FAIL, 2 INCONCLUSIVE.
                         Captures before injecting, requires static baseline, compares
                         8-bit luma changed pixels at |delta| > 24.
  rate | flood           Diagnostic only: compare 30 moves/s control with 60 moves/s flood
                         over 2s and report residual changed-pixel-count; always exits 0.
  cpu                    Diagnostic only: sample openterface-rs CPU for 5s; warn over limit.

Manual destructive verbs (actual sending is gated):
  click                  Absolute left click at centre (press + release).
  type <text>            Type a small ASCII subset; the text is never logged by this script.
  key [hid-usage-hex]    Press + release one USB HID usage (default 04, the 'a' key).

Safety gates for destructive sending:
  Actual click/type/key sending requires BOTH:
    KVM_ALLOW_DESTRUCTIVE=1
    --i-understand-target-is-uncontrolled
  The target may be a lock screen, password prompt, or sensitive session. Missing either
  gate fails closed. DRYRUN=1 may print destructive frames without the gates.

DRYRUN contract:
  For frame-producing commands, DRYRUN=1 performs no device detection, no sleeps, no
  writes, and emits only uppercase hex bytes, one frame per line. The `frames` command
  is the exception that prefixes each stable canonical frame with `LABEL: ` for tests.

Detection / overrides:
  Serial auto-detection only uses /dev/ttyACM* and /dev/ttyUSB* whose USB VID/PID is
  verified as CH9329-compatible Openterface hardware (VID 1A86, PID 7523 or FE0C).
  Use --serial DEV or --video DEV to explicitly override detection.

Environment:
  DRYRUN=1                  Print frames instead of sending where applicable.
  KVM_PACE=SECONDS          Inter-frame delay for CH9329 writes (default 0.004).
  KVM_CPU_MAX=PERCENT       cpu warning threshold (default 25).
  KVM_ALLOW_DESTRUCTIVE=1   Required with the explicit flag for click/type/key sending.
EOF_USAGE
}

# ---- dependency checks -------------------------------------------------------

have_cmd() { command -v "$1" >/dev/null 2>&1; }

preflight() {
  local missing=0 cmd
  for cmd in ffmpeg v4l2-ctl awk od wc; do
    if have_cmd "$cmd"; then
      printf 'preflight: %-9s OK\n' "$cmd"
    else
      printf 'preflight: %-9s MISSING\n' "$cmd" >&2
      missing=1
    fi
  done

  if [ "$missing" -eq 0 ]; then
    echo "preflight: PASS"
  else
    echo "preflight: FAIL" >&2
  fi
  return "$missing"
}

require_cmds() {
  local cmd
  for cmd in "$@"; do
    have_cmd "$cmd" || die "required command not found: $cmd"
  done
}

# ---- device detection --------------------------------------------------------

serial_vid_pid() {
  local node=$1 vid="" pid="" key value base path

  if have_cmd udevadm; then
    while IFS='=' read -r key value; do
      case "$key" in
        ID_VENDOR_ID) vid=${value^^} ;;
        ID_MODEL_ID) pid=${value^^} ;;
      esac
    done < <(udevadm info --query=property --name="$node" 2>/dev/null || true)
    if [ -n "$vid" ] && [ -n "$pid" ]; then
      printf '%s %s\n' "$vid" "$pid"
      return 0
    fi
  fi

  base=$(basename "$node")
  path=$(readlink -f "/sys/class/tty/$base/device" 2>/dev/null || true)
  while [ -n "$path" ] && [ "$path" != "/" ]; do
    if [ -r "$path/idVendor" ] && [ -r "$path/idProduct" ]; then
      vid=$(tr '[:lower:]' '[:upper:]' < "$path/idVendor")
      pid=$(tr '[:lower:]' '[:upper:]' < "$path/idProduct")
      printf '%s %s\n' "$vid" "$pid"
      return 0
    fi
    path=${path%/*}
  done

  return 1
}

is_openterface_ch9329() {
  local ids vid pid
  ids=$(serial_vid_pid "$1") || return 1
  read -r vid pid <<<"$ids"
  [ "$vid" = "1A86" ] && { [ "$pid" = "7523" ] || [ "$pid" = "FE0C" ]; }
}

detect_serial() {
  local s
  for s in /dev/ttyACM* /dev/ttyUSB*; do
    [ -e "$s" ] || continue
    if is_openterface_ch9329 "$s"; then
      printf '%s\n' "$s"
      return 0
    fi
  done
  return 1
}

resolve_serial_for_send() {
  if [ -n "$SERIAL_OVERRIDE" ]; then
    [ -e "$SERIAL_OVERRIDE" ] || return 1
    printf '%s\n' "$SERIAL_OVERRIDE"
    return 0
  fi

  detect_serial
}

get_serial_for_send() {
  local serial
  serial=$(resolve_serial_for_send) || die "no verified Openterface CH9329 serial device found; use --serial DEV to override"
  printf '%s\n' "$serial"
}

detect_video() {
  local v drv
  for v in /dev/video*; do
    [ -e "$v" ] || continue
    drv=$(v4l2-ctl --device="$v" --info 2>/dev/null | grep -i "driver name" || true)
    case "$drv" in *uvcvideo*) ;; *) continue ;; esac
    if v4l2-ctl --device="$v" --list-formats 2>/dev/null | grep -qi mjpg; then
      printf '%s\n' "$v"
      return 0
    fi
  done
  return 1
}

resolve_video() {
  if [ -n "$VIDEO_OVERRIDE" ]; then
    [ -e "$VIDEO_OVERRIDE" ] || return 1
    printf '%s\n' "$VIDEO_OVERRIDE"
    return 0
  fi

  detect_video
}

get_video() {
  local video
  video=$(resolve_video) || die "no UVC/MJPEG capture device found; use --video DEV to override"
  printf '%s\n' "$video"
}

configure_serial() {
  local serial=$1
  if have_cmd stty; then
    stty -F "$serial" 9600 raw -echo -echoe -echok -echoctl -echoke -ixon -ixoff -crtscts 2>/dev/null || true
  fi
}

# ---- CH9329 framing ---------------------------------------------------------
# Every command is `57 AB 00 <CMD> <LEN> <DATA..> <SUM>`, SUM = low byte of the
# additive sum of all preceding bytes. Keep these byte-identical to
# openterface_core::protocol::ch9329.

frame_hex() {
  local cmd=$1 n sum=0 out=""
  shift
  local data=( "$@" )
  local frame=( 0x57 0xAB 0x00 "$cmd" "${#data[@]}" "${data[@]}" )
  for n in "${frame[@]}"; do
    sum=$(( (sum + n) & 0xFF ))
  done
  frame+=( "$sum" )
  for n in "${frame[@]}"; do
    out+=$(printf '%02X ' "$(( n & 0xFF ))")
  done
  printf '%s\n' "${out% }"
}

clamp_abs() {
  local value=$1
  if (( value < ABS_MIN )); then
    printf '%s\n' "$ABS_MIN"
  elif (( value > ABS_MAX )); then
    printf '%s\n' "$ABS_MAX"
  else
    printf '%s\n' "$value"
  fi
}

mouse_abs_hex() {
  local x y buttons wheel
  x=$(clamp_abs "$1")
  y=$(clamp_abs "$2")
  buttons=$3
  wheel=$4
  frame_hex 0x04 \
    0x02 \
    "$(( buttons & 0xFF ))" \
    "$(( x & 0xFF ))" \
    "$(( (x >> 8) & 0xFF ))" \
    "$(( y & 0xFF ))" \
    "$(( (y >> 8) & 0xFF ))" \
    "$(( wheel & 0xFF ))"
}

mouse_rel_hex() {
  local dx=$1 dy=$2 buttons=$3 wheel=$4
  frame_hex 0x05 \
    0x01 \
    "$(( buttons & 0xFF ))" \
    "$(( dx & 0xFF ))" \
    "$(( dy & 0xFF ))" \
    "$(( wheel & 0xFF ))"
}

keyboard_hex() {
  local modifiers=$1 i key
  shift || true
  local keys=( "$@" )
  local data=( "$(( modifiers & 0xFF ))" 0x00 )
  for i in 0 1 2 3 4 5; do
    key=${keys[$i]:-0}
    data+=( "$(( key & 0xFF ))" )
  done
  frame_hex 0x02 "${data[@]}"
}

keyboard_release_hex() {
  keyboard_hex 0x00
}

emit_labeled_frame() {
  local label=$1 hex=$2
  printf '%s: %s\n' "$label" "$hex"
}

canonical_frames() {
  emit_labeled_frame MOVE_ABS "$(mouse_abs_hex "$MOVE_END_X" "$MOVE_END_Y" 0x00 0)"
  emit_labeled_frame CLICK_PRESS "$(mouse_abs_hex "$CLICK_X" "$CLICK_Y" 0x01 0)"
  emit_labeled_frame CLICK_RELEASE "$(mouse_abs_hex "$CLICK_X" "$CLICK_Y" 0x00 0)"
  emit_labeled_frame TYPE_A_PRESS "$(keyboard_hex 0x00 0x04)"
  emit_labeled_frame TYPE_A_RELEASE "$(keyboard_release_hex)"
  emit_labeled_frame KEY_A_PRESS "$(keyboard_hex 0x00 0x04)"
  emit_labeled_frame KEY_A_RELEASE "$(keyboard_release_hex)"
  emit_labeled_frame MOVE_REL "$(mouse_rel_hex 5 -3 0x00 0)"
  emit_labeled_frame ABS_CENTRE "$(mouse_abs_hex "$ABS_CENTRE" "$ABS_CENTRE" 0x00 0)"
}

move_frames() {
  mouse_abs_hex "$MOVE_START_X" "$MOVE_START_Y" 0x00 0
  mouse_abs_hex "$MOVE_END_X" "$MOVE_END_Y" 0x00 0
}

key_frames() {
  local usage=${1:-0x04}
  keyboard_hex 0x00 "$usage"
  keyboard_release_hex
}

hid_for_char() {
  local ch=$1 ord usage
  case "$ch" in
    [a-z])
      printf -v ord '%d' "'$ch"
      usage=$(( ord - 97 + 0x04 ))
      printf '0x00 %s\n' "$usage"
      ;;
    [A-Z])
      printf -v ord '%d' "'$ch"
      usage=$(( ord - 65 + 0x04 ))
      printf '0x02 %s\n' "$usage"
      ;;
    [1-9])
      printf -v ord '%d' "'$ch"
      usage=$(( ord - 49 + 0x1E ))
      printf '0x00 %s\n' "$usage"
      ;;
    0)
      printf '0x00 0x27\n'
      ;;
    ' ')
      printf '0x00 0x2C\n'
      ;;
    *)
      return 1
      ;;
  esac
}

type_frames() {
  local text=$1 i ch pair modifiers usage
  for (( i = 0; i < ${#text}; i++ )); do
    ch=${text:i:1}
    pair=$(hid_for_char "$ch") || return 1
    read -r modifiers usage <<<"$pair"
    keyboard_hex "$modifiers" "$usage"
    keyboard_release_hex
  done
}

parse_hid_usage() {
  local raw=${1:-04}
  case "$raw" in
    0x[0-9A-Fa-f]|0x[0-9A-Fa-f][0-9A-Fa-f]|0X[0-9A-Fa-f]|0X[0-9A-Fa-f][0-9A-Fa-f])
      printf '%s\n' "$(( raw ))"
      ;;
    [0-9A-Fa-f]|[0-9A-Fa-f][0-9A-Fa-f])
      printf '%s\n' "$(( 16#$raw ))"
      ;;
    *)
      return 1
      ;;
  esac
}

hex_to_escapes() {
  local hex=$1 byte out=""
  for byte in $hex; do
    out+="\\x$byte"
  done
  printf '%s' "$out"
}

write_hex_frame_to_fd() {
  local fd=$1 hex=$2 escaped
  escaped=$(hex_to_escapes "$hex")
  printf '%b' "$escaped" >&"$fd"
}

send_hex_frame() {
  local serial=$1 hex=$2
  if is_dryrun; then
    printf '%s\n' "$hex"
    return 0
  fi

  exec {serial_fd}>"$serial"
  write_hex_frame_to_fd "$serial_fd" "$hex"
  exec {serial_fd}>&-
  sleep "$PACE"
}

send_hex_frames() {
  local serial=$1 hex
  shift
  configure_serial "$serial"
  for hex in "$@"; do
    send_hex_frame "$serial" "$hex"
  done
}

require_destructive_ok() {
  cat >&2 <<'EOF_WARNING'
********************************************************************************
DANGER: this command injects destructive keyboard/click input into an uncontrolled
Openterface target. The target may be a lock screen, password prompt, production
machine, or sensitive user session. Do not continue unless you control the target.
********************************************************************************
EOF_WARNING

  [ "${KVM_ALLOW_DESTRUCTIVE:-0}" = "1" ] || die "refusing destructive input: set KVM_ALLOW_DESTRUCTIVE=1"
  [ "$DESTRUCTIVE_OK_FLAG" -eq 1 ] || die "refusing destructive input: pass --i-understand-target-is-uncontrolled"
}

# ---- capture / metric helpers -----------------------------------------------

capture_gray_od_text() {
  local video=$1 warmup=$2 vf out count
  vf="select=gte(n\\,$warmup),format=gray"
  for _ in 1 2 3; do
    if out=$(ffmpeg -hide_banner -loglevel error -f v4l2 -input_format mjpeg \
      -video_size "${WIDTH}x${HEIGHT}" -i "$video" -vf "$vf" -frames:v 1 \
      -f rawvideo -pix_fmt gray - 2>/dev/null | od -An -v -tu1); then
      count=$(printf '%s\n' "$out" | wc -w | tr -d '[:space:]')
      if [ "$count" -eq "$FRAME_PIXELS" ]; then
        printf '%s\n' "$out"
        return 0
      fi
    fi
    sleep 0.1
  done
  return 1
}

pixel_diff_texts() {
  local first=$1 second=$2
  { printf '%s\n' "$first"; printf '%s\n' "$second"; } | awk \
    -v pixels="$FRAME_PIXELS" \
    -v luma="$LUMA_THRESHOLD" '
      function abs(v) { return v < 0 ? -v : v }
      {
        for (i = 1; i <= NF; i++) {
          v = $i + 0
          idx = count % pixels
          frame = int(count / pixels)
          count++
          if (frame == 0) {
            baseline[idx] = v
          } else if (frame == 1) {
            if (abs(v - baseline[idx]) > luma) changed++
            if (idx == pixels - 1) {
              print changed
              exit 0
            }
          }
        }
      }
      END {
        if (count < 2 * pixels) exit 1
      }
    '
}

close_diff_coproc() {
  local diff_in=$1 diff_out=$2 diff_pid=$3
  exec {diff_in}>&- || true
  exec {diff_out}<&- || true
  wait "$diff_pid" 2>/dev/null || true
}

run_capture() {
  local out=${1:-}
  [ -n "$out" ] || die "usage: kvm-debug capture <out.jpg>"
  if is_dryrun; then
    return 0
  fi

  require_cmds ffmpeg v4l2-ctl
  local video
  video=$(get_video)
  ffmpeg -hide_banner -loglevel error -f v4l2 -input_format mjpeg \
    -video_size "${WIDTH}x${HEIGHT}" -i "$video" -frames:v 1 -y "$out"
  echo "capture: wrote $out"
}

run_diff() {
  if is_dryrun; then
    move_frames
    return 0
  fi

  require_cmds ffmpeg v4l2-ctl awk od wc
  local video serial start_frame end_frame text tag n0 changed threshold diff_in diff_out diff_pid
  video=$(get_video)
  serial=$(get_serial_for_send)
  configure_serial "$serial"
  start_frame=$(mouse_abs_hex "$MOVE_START_X" "$MOVE_START_Y" 0x00 0)
  end_frame=$(mouse_abs_hex "$MOVE_END_X" "$MOVE_END_Y" 0x00 0)

  coproc DIFF_AWK { awk -v pixels="$FRAME_PIXELS" -v luma="$LUMA_THRESHOLD" '
    function abs(v) { return v < 0 ? -v : v }
    {
      for (i = 1; i <= NF; i++) {
        v = $i + 0
        idx = count % pixels
        frame = int(count / pixels)
        count++
        if (frame == 0) {
          baseline_a[idx] = v
        } else if (frame == 1) {
          baseline_b[idx] = v
          if (abs(v - baseline_a[idx]) > luma) n0++
          if (idx == pixels - 1) {
            print "N0", n0
            fflush()
            delete baseline_a
          }
        } else if (frame == 2) {
          if (abs(v - baseline_b[idx]) > luma) changed++
          if (idx == pixels - 1) {
            print "CHANGED", changed
            fflush()
            exit 0
          }
        }
      }
    }
    END {
      if (count < 2 * pixels) {
        print "ERROR short_input"
        fflush()
        exit 1
      }
    }
  '; }
  diff_in=${DIFF_AWK[1]}
  diff_out=${DIFF_AWK[0]}
  diff_pid=$DIFF_AWK_PID

  text=$(capture_gray_od_text "$video" 5) || { close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"; die "diff capture failed"; }
  printf '%s\n' "$text" >&"$diff_in"
  sleep 0.2
  text=$(capture_gray_od_text "$video" 5) || { close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"; die "diff baseline recapture failed"; }
  printf '%s\n' "$text" >&"$diff_in"

  if ! read -r tag n0 <&"$diff_out"; then
    close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"
    die "diff baseline metric failed"
  fi
  if [ "$tag" != "N0" ]; then
    close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"
    die "diff baseline metric failed"
  fi

  printf 'diff: baseline_changed_pixels=%s static_threshold=%s\n' "$n0" "$STATIC_THRESHOLD"
  if [ "$n0" -gt "$STATIC_THRESHOLD" ]; then
    echo "diff: INCONCLUSIVE screen_not_static"
    close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"
    return 2
  fi

  send_hex_frames "$serial" "$start_frame" "$end_frame"
  sleep 0.3
  text=$(capture_gray_od_text "$video" 5) || { close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"; die "diff comparison capture failed"; }
  printf '%s\n' "$text" >&"$diff_in"

  if ! read -r tag changed <&"$diff_out"; then
    close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"
    die "diff comparison metric failed"
  fi
  close_diff_coproc "$diff_in" "$diff_out" "$diff_pid"
  if [ "$tag" != "CHANGED" ]; then
    die "diff comparison metric failed"
  fi

  threshold=$(( 5 * n0 ))
  if [ "$threshold" -lt 50 ]; then
    threshold=50
  fi
  printf 'diff: changed_pixels=%s pass_threshold=%s\n' "$changed" "$threshold"
  if [ "$changed" -gt "$threshold" ]; then
    echo "diff: PASS"
    return 0
  fi

  echo "diff: FAIL"
  return 1
}

burst_frames() {
  local rate=$1 count i
  count=$(( rate * 2 ))
  for (( i = 0; i < count; i++ )); do
    if (( i % 2 == 0 )); then
      mouse_abs_hex "$MOVE_START_X" "$MOVE_START_Y" 0x00 0
    else
      mouse_abs_hex "$MOVE_END_X" "$MOVE_END_Y" 0x00 0
    fi
  done
  mouse_abs_hex "$MOVE_END_X" "$MOVE_END_Y" 0x00 0
}

send_rate_burst() {
  local serial=$1 rate=$2 frame_a=$3 frame_b=$4 count i period serial_fd
  count=$(( rate * 2 ))
  period=$(awk -v rate="$rate" -v pace="$PACE" 'BEGIN { p = 1 / rate; if (p < pace) p = pace; printf "%.6f", p }')
  exec {serial_fd}>"$serial"
  for (( i = 0; i < count; i++ )); do
    if (( i % 2 == 0 )); then
      write_hex_frame_to_fd "$serial_fd" "$frame_a"
    else
      write_hex_frame_to_fd "$serial_fd" "$frame_b"
    fi
    sleep "$period"
  done
  write_hex_frame_to_fd "$serial_fd" "$frame_b"
  exec {serial_fd}>&-
}

run_rate_once() {
  local rate=$1 video=$2 serial=$3 frame_a=$4 frame_b=$5 pid early late residual planned
  planned=$(( rate * 2 + 1 ))
  send_rate_burst "$serial" "$rate" "$frame_a" "$frame_b" &
  pid=$!
  sleep 0.2
  if ! early=$(capture_gray_od_text "$video" 0); then
    wait "$pid" 2>/dev/null || true
    printf 'rate/flood: rate_hz=%s sent_frames=unavailable residual_changed_pixels=unavailable diagnostic_error=capture_t0_2\n' "$rate"
    return 0
  fi
  sleep 2.0
  if ! late=$(capture_gray_od_text "$video" 0); then
    wait "$pid" 2>/dev/null || true
    printf 'rate/flood: rate_hz=%s sent_frames=unavailable residual_changed_pixels=unavailable diagnostic_error=capture_t2_2\n' "$rate"
    return 0
  fi
  wait "$pid" 2>/dev/null || true
  residual=$(pixel_diff_texts "$early" "$late") || residual="unavailable"
  printf 'rate/flood: rate_hz=%s planned_frames=%s residual_changed_pixels=%s diagnostic_only\n' "$rate" "$planned" "$residual"
}

run_rate_diagnostic() {
  if is_dryrun; then
    burst_frames 30
    burst_frames 60
    return 0
  fi

  local video serial frame_a frame_b
  if ! have_cmd ffmpeg || ! have_cmd v4l2-ctl || ! have_cmd awk || ! have_cmd od || ! have_cmd wc; then
    warn "rate/flood diagnostic unavailable: ffmpeg, v4l2-ctl, awk, od, and wc are required"
    return 0
  fi
  if ! video=$(resolve_video); then
    warn "rate/flood diagnostic unavailable: no capture device"
    return 0
  fi
  if ! serial=$(resolve_serial_for_send); then
    warn "rate/flood diagnostic unavailable: no verified CH9329 serial device (or --serial override)"
    return 0
  fi

  configure_serial "$serial"
  frame_a=$(mouse_abs_hex "$MOVE_START_X" "$MOVE_START_Y" 0x00 0)
  frame_b=$(mouse_abs_hex "$MOVE_END_X" "$MOVE_END_Y" 0x00 0)
  run_rate_once 30 "$video" "$serial" "$frame_a" "$frame_b"
  run_rate_once 60 "$video" "$serial" "$frame_a" "$frame_b"
  echo "rate/flood: diagnostic complete (exit status forced to 0)"
  return 0
}

find_openterface_pids() {
  local comm_file pid comm first_arg base
  for comm_file in /proc/[0-9]*/comm; do
    [ -r "$comm_file" ] || continue
    pid=${comm_file#/proc/}
    pid=${pid%/comm}
    comm=$(<"$comm_file") || continue
    first_arg=""
    if [ -r "/proc/$pid/cmdline" ]; then
      IFS= read -r first_arg < <(tr '\0' '\n' < "/proc/$pid/cmdline" 2>/dev/null || true)
    fi
    base=${first_arg##*/}
    if [ "$comm" = "openterface-rs" ] || [ "$base" = "openterface-rs" ]; then
      printf '%s\n' "$pid"
    fi
  done
}

proc_ticks() {
  local total=0 pid stat tail fields
  for pid in "$@"; do
    [ -r "/proc/$pid/stat" ] || continue
    IFS= read -r stat < "/proc/$pid/stat" || continue
    tail=${stat##*) }
    read -r -a fields <<<"$tail"
    if [ "${#fields[@]}" -gt 12 ]; then
      total=$(( total + fields[11] + fields[12] ))
    fi
  done
  printf '%s\n' "$total"
}

system_ticks() {
  local -a stat_fields
  local i total=0
  read -r -a stat_fields < /proc/stat
  for (( i = 1; i < ${#stat_fields[@]}; i++ )); do
    total=$(( total + stat_fields[i] ))
  done
  printf '%s\n' "$total"
}

run_cpu_diagnostic() {
  if is_dryrun; then
    return 0
  fi

  local pids=() p0 p1 s0 s1 proc_delta system_delta ncpu clk_tck elapsed percent pid_list
  mapfile -t pids < <(find_openterface_pids)
  if [ "${#pids[@]}" -eq 0 ]; then
    echo "cpu: no openterface-rs process found; diagnostic_only"
    return 0
  fi

  p0=$(proc_ticks "${pids[@]}")
  s0=$(system_ticks)
  sleep 5
  p1=$(proc_ticks "${pids[@]}")
  s1=$(system_ticks)
  proc_delta=$(( p1 - p0 ))
  system_delta=$(( s1 - s0 ))
  if [ "$proc_delta" -lt 0 ]; then
    proc_delta=0
  fi
  if [ "$system_delta" -lt 1 ]; then
    system_delta=1
  fi

  ncpu=$(getconf _NPROCESSORS_ONLN 2>/dev/null || printf '1')
  clk_tck=$(getconf CLK_TCK 2>/dev/null || printf '100')
  elapsed=$(awk -v sd="$system_delta" -v hz="$clk_tck" -v ncpu="$ncpu" 'BEGIN { printf "%.2f", sd / (hz * ncpu) }')
  percent=$(awk -v pd="$proc_delta" -v sd="$system_delta" -v ncpu="$ncpu" 'BEGIN { printf "%.1f", 100 * pd * ncpu / sd }')
  pid_list=$(IFS=,; printf '%s' "${pids[*]}")
  printf 'cpu: pids=%s sample_seconds=%s cpu_percent=%s limit=%s diagnostic_only\n' "$pid_list" "$elapsed" "$percent" "$CPU_MAX"
  if awk -v p="$percent" -v limit="$CPU_MAX" 'BEGIN { exit !(p > limit) }'; then
    printf 'cpu: WARN above KVM_CPU_MAX=%s\n' "$CPU_MAX"
  fi
  return 0
}

run_move() {
  if is_dryrun; then
    move_frames
    return 0
  fi

  local serial start_frame end_frame
  serial=$(get_serial_for_send)
  start_frame=$(mouse_abs_hex "$MOVE_START_X" "$MOVE_START_Y" 0x00 0)
  end_frame=$(mouse_abs_hex "$MOVE_END_X" "$MOVE_END_Y" 0x00 0)
  send_hex_frames "$serial" "$start_frame" "$end_frame"
  echo "move: sent absolute mouse moves; ended at ${MOVE_END_X},${MOVE_END_Y}"
}

run_click() {
  local serial press release
  press=$(mouse_abs_hex "$CLICK_X" "$CLICK_Y" 0x01 0)
  release=$(mouse_abs_hex "$CLICK_X" "$CLICK_Y" 0x00 0)
  if is_dryrun; then
    printf '%s\n%s\n' "$press" "$release"
    return 0
  fi

  require_destructive_ok
  serial=$(get_serial_for_send)
  send_hex_frames "$serial" "$press" "$release"
  echo "click: sent press/release"
}

run_type() {
  local text frames serial
  local -a frame_array
  if [ "$#" -lt 1 ]; then
    die "usage: kvm-debug type <text>"
  fi
  text=$*
  frames=$(type_frames "$text") || die "unsupported character for HID typing"
  if is_dryrun; then
    printf '%s\n' "$frames"
    return 0
  fi

  require_destructive_ok
  serial=$(get_serial_for_send)
  mapfile -t frame_array <<<"$frames"
  send_hex_frames "$serial" "${frame_array[@]}"
  echo "type: sent HID reports"
}

run_key() {
  local usage serial press release
  usage=$(parse_hid_usage "${1:-04}") || die "usage must be one or two hex digits (for example: 04)"
  press=$(keyboard_hex 0x00 "$usage")
  release=$(keyboard_release_hex)
  if is_dryrun; then
    printf '%s\n%s\n' "$press" "$release"
    return 0
  fi

  require_destructive_ok
  serial=$(get_serial_for_send)
  send_hex_frames "$serial" "$press" "$release"
  echo "key: sent press/release"
}

show_devices() {
  if is_dryrun; then
    return 0
  fi

  local video serial
  if video=$(resolve_video); then
    printf 'video:  %s\n' "$video"
  else
    printf 'video:  (none)\n'
  fi

  if serial=$(resolve_serial_for_send); then
    if [ -n "$SERIAL_OVERRIDE" ]; then
      printf 'serial: %s (override)\n' "$serial"
    else
      printf 'serial: %s (verified VID/PID)\n' "$serial"
    fi
  else
    printf 'serial: (none verified; use --serial DEV to override)\n'
  fi
}

main() {
  local args=() cmd
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --serial)
        [ "$#" -ge 2 ] || die "--serial requires a device path"
        SERIAL_OVERRIDE=$2
        shift 2
        ;;
      --video)
        [ "$#" -ge 2 ] || die "--video requires a device path"
        VIDEO_OVERRIDE=$2
        shift 2
        ;;
      --i-understand-target-is-uncontrolled)
        DESTRUCTIVE_OK_FLAG=1
        shift
        ;;
      --)
        shift
        while [ "$#" -gt 0 ]; do
          args+=( "$1" )
          shift
        done
        ;;
      *)
        args+=( "$1" )
        shift
        ;;
    esac
  done

  set -- "${args[@]}"
  cmd=${1:-}
  if [ "$#" -gt 0 ]; then
    shift
  fi

  case "$cmd" in
    preflight) preflight ;;
    devices) show_devices ;;
    capture) run_capture "$@" ;;
    frames) canonical_frames ;;
    frame-bytes) mouse_abs_hex "$ABS_CENTRE" "$ABS_CENTRE" 0x00 0 ;;
    move) run_move ;;
    diff) run_diff ;;
    rate|flood) run_rate_diagnostic ;;
    cpu) run_cpu_diagnostic ;;
    click) run_click ;;
    type) run_type "$@" ;;
    key) run_key "$@" ;;
    ""|-h|--help|help) usage ;;
    *) die "unknown command '$cmd' (try: kvm-debug help)" ;;
  esac
}

main "$@"
