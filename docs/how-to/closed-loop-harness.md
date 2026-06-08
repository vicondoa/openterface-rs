# Closed-loop hardware harness

`tools/kvm-debug.sh` verifies a **real** Openterface with **no human** watching a
window. It talks to both USB endpoints directly: it injects raw CH9329 input on
the serial node and grabs MJPEG frames from the capture node, so input and video
can be checked in a closed loop (inject → capture → compare). It is
dependency-light (bash + coreutils + `v4l-utils` + `ffmpeg`) so it runs headless
over SSH — for example inside the work-ssd VM.

```bash
tools/kvm-debug.sh [--serial DEV] [--video DEV] <command> [args]
```

## Safety first

The target screen is **uncontrolled** — it may be a lock screen, password
prompt, or sensitive session. Therefore:

- **Automated verification uses non-destructive mouse moves only.** The `diff`
  check captures a baseline *before* injecting and only moves the mouse.
- **Destructive verbs (`click`/`type`/`key`) are manual-only.** Actually sending
  them requires **both** `KVM_ALLOW_DESTRUCTIVE=1` **and** the explicit flag
  `--i-understand-target-is-uncontrolled`; otherwise the script prints a loud
  warning and **fails closed**. (Under `DRYRUN=1` their frames may be *printed*
  without the gates — printing sends nothing.)
- The injected text/keys are **never logged** by the script.
- Auto-detection only drives a serial node whose USB VID/PID is a verified
  CH9329 (`1A86:7523`/`1A86:FE0C`); use `--serial`/`--video` to override.

## Commands

### Setup

| Command | Purpose |
|---------|---------|
| `preflight` | Check required tools (`ffmpeg`, `v4l2-ctl`, `awk`, `od`, `wc`). |
| `devices` | Print the detected UVC/MJPEG video + verified CH9329 serial nodes. |
| `capture <out.jpg>` | Grab one 1280×720 MJPEG frame to a file. |

### Dry-run framing (no device)

| Command | Purpose |
|---------|---------|
| `frames` | With `DRYRUN=1`, print canonical labelled CH9329 frames (`LABEL: HEXBYTES`). |
| `frame-bytes` | With `DRYRUN=1`, print the abs-centre frame only. |

`DRYRUN=1` performs no device detection, no sleeps, no writes, and emits only
uppercase hex bytes. A hardware-free Rust test
(`crates/openterface-cli/tests/harness_framing.rs`) asserts the `frames` output
stays **byte-identical** to the `openterface-core` builders, so the harness can
never silently drift from the protocol code.

### Non-destructive automated assertions

| Command | Exit codes | Purpose |
|---------|-----------|---------|
| `move` | 0 | Corner→corner absolute mouse moves, ending bottom-right. |
| `diff` | `0` PASS / `1` FAIL / `2` INCONCLUSIVE | Capture baseline → inject move → capture → pixel-diff proves input round-trips. |
| `rate` \| `flood` | 0 (diagnostic) | Compare 30 vs 60 moves/s residual motion (pacing overrun probe). |
| `cpu` | 0 (diagnostic) | Sample `openterface-rs` CPU over 5 s; warn over `KVM_CPU_MAX`. |

**`diff` thresholds** (deterministic, on 8-bit luma of a 1280×720 frame): a pixel
is "changed" if |Δ| > 24. A static baseline is required — if the baseline noise
`N0` (changed pixels between two baseline frames 200 ms apart) exceeds **2000**,
the screen is not static enough and `diff` exits **2 (INCONCLUSIVE)**. Otherwise
the corner→corner move (ending at a fixed corner) is injected, the script sleeps
**300 ms** for capture propagation, discards 5 warm-up frames, captures one
comparison frame, and **PASSes** iff the changed-pixel count vs the baseline
exceeds `max(50, 5×N0)`.

`rate`/`flood` are **diagnostic only** (never a hard pass): residual cursor drift
after a 60 Hz burst depends on the target OS, so it is reported, not asserted.

### Manual destructive verbs (gated)

| Command | Purpose |
|---------|---------|
| `click` | Absolute left click at centre (press + release). |
| `type <text>` | Type a small ASCII subset (text is never logged). |
| `key [hex]` | Press + release one HID usage (default `04` = `a`). |

```bash
# This WILL type on whatever is focused on the target — only if you control it:
KVM_ALLOW_DESTRUCTIVE=1 tools/kvm-debug.sh --i-understand-target-is-uncontrolled type "hello"
```

## Environment

| Variable | Default | Effect |
|----------|--------:|--------|
| `DRYRUN` | `0` | `1` = print frames, never send. |
| `KVM_PACE` | `0.004` | Seconds between CH9329 writes (chip rate limit). |
| `KVM_CPU_MAX` | `25` | `cpu` warning threshold (percent). |
| `KVM_ALLOW_DESTRUCTIVE` | _(unset)_ | Required (with the flag) for destructive sending. |

## Typical headless run

```bash
tools/kvm-debug.sh preflight
tools/kvm-debug.sh devices
tools/kvm-debug.sh diff        # 0 = input round-tripped to the target
tools/kvm-debug.sh cpu         # idle-throttle sanity
```

This is the loop used to validate openterface-rs on real hardware in the
work-ssd VM.
