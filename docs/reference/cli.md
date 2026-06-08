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
| `--no-video` | Input-only / headless: forward keyboard/mouse with a blank window, no video. |
| `--dummy` | No device; GUI with a test pattern (development/CI). |
| `--debug` | Log each forwarded input event. |

A display session needs the `hardware` feature (see
[build](../how-to/build.md)); a binary built without it can still run `scan`
and `status`.

> **Parity note (exit codes).** Unlike the C++ CLI (which exits `0` even on
> runtime failure), openterface-rs exits **`1`** when a device is not found, a
> connection fails, or a reset fails — silently exiting `0` on failure is a
> scripting footgun. Usage errors exit `2`; `--help`/`--version` exit `0`.

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

> **Parity note.** The C++ `status` prints in-process connection state; the Rust
> `status` is detection-based (it reports what is currently present). Tracked for
> the parity gate.

## `reset`

CH9329 factory reset.

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
