# Device IDs & node selection

The Openterface presents two independent USB endpoints over one cable.

## USB identities

| Half | Chip | USB VID:PID | Linux node |
|------|------|-------------|------------|
| Input | CH9329 USB-serial HID | `1A86:7523` (classic) or `1A86:FE0C` (newer firmware) | `/dev/ttyACM*` |
| Video | MS2109 HDMI capture | `345F:2109` (validated), alt `534D:2109` (MacroSilicon upstream) | `/dev/video*` |

These constants live in `openterface_core::device`.

## Video node selection

A single physical capture can expose **several** `/dev/video*` nodes, and a VM
adds more. The application's `SysfsScanner` selects the node whose **card name
contains `Openterface`** or whose USB **VID/PID** matches the MS2109
(`345F:2109` / `534D:2109`). This already **skips the virtio-media `/dev/video0`
decoder adapter** (different card name and modalias) and the secondary/metadata
nodes.

The [closed-loop harness](../how-to/closed-loop-harness.md) uses a stricter
heuristic — it additionally requires the **`uvcvideo`** driver **and** an
advertised **`MJPG`** format — which is the most robust selection when the card
name is ambiguous. (Bringing that exact `uvcvideo` + `MJPG` check into the
application scanner is tracked for the parity gate.)

## Serial node selection

The CH9329 enumerates as `/dev/ttyACM*` (occasionally `/dev/ttyUSB*`). Selection
verifies the USB **VID/PID** (`1A86:7523`/`1A86:FE0C`) via sysfs before a node is
used as an input-injection target, so an unrelated serial device is never driven
by accident. Explicit `--serial`/`--video` paths override auto-detection.

## Permissions

Access to these nodes for a non-root user is granted by the shipped udev rules —
see [permissions & udev](../how-to/permissions-udev.md).
