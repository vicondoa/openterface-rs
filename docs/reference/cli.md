# CLI reference

The binary is `openterface-rs`. Exactly one subcommand is required.

```
openterface-rs [GLOBAL OPTIONS] <COMMAND> [COMMAND OPTIONS]
```

## Global options

| Option | Description |
|--------|-------------|
| `-v`, `--verbose` | Verbose output. |
| `--version` | Print the version (`1.0.0`) and exit. |
| `-h`, `--help` | Print help and exit. |

Logging verbosity is also controlled by `RUST_LOG` (see
[environment variables](env-vars.md)).

## `connect`

Start a KVM session: show the target's video and forward keyboard/mouse.

```
openterface-rs connect [--video PATH] [--serial PATH]
                       [--no-video] [--no-serial] [--dummy] [--debug]
```

| Option | Description |
|--------|-------------|
| `--video PATH` | Capture device path. Auto-detected if omitted. |
| `--serial PATH` | CH9329 serial path. Auto-detected if omitted. |
| `--no-serial` | Show video only; do not forward input. |
| `--no-video` | Input-only / headless: forward keyboard/mouse with a window showing a static test pattern (no target video). |
| `--dummy` | No device; GUI with a test pattern (development/CI). |
| `--debug` | Log each forwarded input event (keyboard events are redacted — no key identity is logged). |

A display session needs the `hardware` feature (see
[build](../how-to/build.md)); a binary built without it can still run `scan`
and `status`.

### Focused paste

When the session window has keyboard focus, `Ctrl+Shift+V` reads the local
regular Wayland clipboard and types supported text into the target through the CH9329
keyboard path. The shortcut is host-local while paste is enabled: it is swallowed
by openterface-rs and is **not** forwarded to the target.

Middle-click behavior is separate: by default middle-clicks are forwarded to the
target unchanged, matching a normal KVM/mouse path. Set
`OPENTERFACE_MIDDLE_CLICK_PASTE=primary` to make middle-click type the host
primary selection, or `clipboard` to make middle-click type the regular
clipboard instead.

Paste is local-clipboard-to-target typing, not target clipboard synchronization.
It uses the normal focused-client Wayland clipboard path (no XWayland and no
global/data-control clipboard scrape). The first pass uses the existing US-layout
ASCII mapper: newline becomes Enter, tab becomes Tab, unsupported characters are
skipped and reported by count. Large pastes are capped by
`OPENTERFACE_PASTE_MAX_CHARS`; press Escape, unfocus, or close the window to
abort pending paste frames. Paste status is logged and reflected in the window
title. If your compositor or fullscreen mode hides titles, use logs for paste
feedback.

Set `OPENTERFACE_PASTE_SHORTCUT` to another modifier+`V` chord if `Ctrl+Shift+V`
conflicts with your target workflow; for example `ctrl-alt-shift-v` restores the
original 4-key chord. The configured shortcut is never forwarded to the target
while paste is enabled.

> **Behavior note (exit codes).** openterface-rs exits **`1`** when a device is
> not found, a connection fails, or a reset fails. Usage errors exit `2`;
> `--help`/`--version` exit `0`.

> **Behavior note (no device).** When no device is found, openterface-rs
> **errors** (exit `1`) with a hint to run `scan`. Use `connect --dummy` for a
> deviceless window.

## `scan`

Enumerate Openterface devices (pure sysfs; no connection, no hardware feature
required).

```
openterface-rs scan
```

## `status`

Show detected device status.

```
openterface-rs status
```

> **Behavior note.** `status` is detection-based: it reports what is currently
> present.

## `reset`

CH9329 **factory reset**: pulses the RTS line high for ~4 s (hardware reset),
then reconfigures the chip to mode `0x82` / 115200 baud (software reconfigure).
The command **blocks for ~6 s**.

```
openterface-rs reset --serial PATH
```

`--serial` is required; if omitted, the command prints usage and exits **`1`**
(it is validated in the handler, not by the parser). Requires the `hardware`
feature.

## Exit codes

| Code | Meaning |
|-----:|---------|
| `0` | Success (including `--help`/`--version`). |
| `1` | Runtime failure (no device, connection/reset failed, `reset` missing `--serial`). |
| `2` | Parser usage error (unknown/invalid flags, missing required subcommand). |

The canonical `--help` text is captured in
[`cli-help.txt`](cli-help.txt); regenerate it with
`cargo run -p openterface-cli -- --help > docs/reference/cli-help.txt`.
