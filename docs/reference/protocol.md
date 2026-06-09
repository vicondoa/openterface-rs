# CH9329 protocol reference

This describes the CH9329 serial command framing that openterface-rs implements
in `openterface_core::protocol::ch9329`. It is independently authored from
observed hardware behavior; no third-party source is copied. The CH9329 is a
USB-serial HID bridge: the host sends framed commands over the serial link and
the chip emits USB HID reports to the target.

## Frame format

Every command is:

```
57 AB 00 <CMD> <LEN> <DATA…> <SUM>
```

| Field | Bytes | Meaning |
|-------|------:|---------|
| Prefix | `57 AB 00` | Fixed frame header. |
| `CMD` | 1 | Command opcode (below). |
| `LEN` | 1 | Number of `DATA` bytes. |
| `DATA` | `LEN` | Command payload. |
| `SUM` | 1 | Checksum: low byte of the additive sum of **all** preceding bytes. |

The link is **115200 8N1** with a **fallback to 9600**: connection negotiation
tries 115200 first, then 9600. Some firmware does not answer `GET_INFO`; a
missing response is harmless and must not be treated as an error.

## Opcodes

| `CMD` | Name | Payload |
|------:|------|---------|
| `0x01` | `GET_INFO` | _(none)_ |
| `0x02` | `KEYBOARD` | `<mod> 00 <k1> <k2> <k3> <k4> <k5> <k6>` |
| `0x04` | `MOUSE_ABS` | `02 <buttons> <xLo> <xHi> <yLo> <yHi> <wheel>` |
| `0x05` | `MOUSE_REL` | `01 <buttons> <dx> <dy> <wheel>` |
| `0x0F` | `RESET` | _(none)_ — software reset |

### Keyboard (`0x02`)

`<mod>` is a modifier bitmask; the second byte is reserved (`00`); up to six HID
usage codes follow. An all-zero report (`00 00 00 00 00 00 00 00`) releases all
keys. Modifier bits:

| Bit | Modifier |
|----:|----------|
| `0x01` | Left Ctrl |
| `0x02` | Left Shift |
| `0x04` | Left Alt |
| `0x08` | Left Meta (GUI) |

Ctrl+Alt+Del is HID usage `0x4C` with modifiers `0x05` (Ctrl+Alt).

### Absolute mouse (`0x04`)

`X` and `Y` are `0..=4095`, little-endian, mapping the full capture area
regardless of resolution. `<buttons>` is a button bitmask; `<wheel>` is a signed
tick (`0` for none).

### Relative mouse (`0x05`)

`<dx>`/`<dy>` are signed 8-bit deltas; `<wheel>` is a signed scroll tick
(`0x01` up, `0xFF` down).

## Checksum

```
sum = 0
for b in all_bytes_before_checksum:
    sum = (sum + b) & 0xFF
```

## Worked examples (golden vectors)

These exact byte strings are asserted in the crate's tests.

| Meaning | Frame |
|---------|-------|
| Absolute move to (100, 200), left button | `57 AB 00 04 07 02 01 64 00 C8 00 00 3C` |
| Relative move dx=+5 dy=−3, left button | `57 AB 00 05 05 01 01 05 FD 00 10` |
| Press key `a` (HID `0x04`) | `57 AB 00 02 08 00 00 04 00 00 00 00 00 10` |

The [closed-loop harness](../how-to/closed-loop-harness.md) emits the same
frames under `DRYRUN=1`, and a hardware-free test asserts the harness output
stays byte-identical to these builders.
