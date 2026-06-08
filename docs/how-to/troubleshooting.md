# Troubleshooting

## Device not found / `scan` shows nothing

- Install the [udev rules](permissions-udev.md) and **replug** the device
  (rules only apply to devices added after they load).
- Confirm the kernel sees it: `lsusb | grep -iE '1a86|345f|534d'`.
- The CH9329 should appear as `/dev/ttyACM*`; the capture as `/dev/video*`.

## `Permission denied` on `/dev/ttyACM0` or `/dev/videoN`

You are not granted access. On a desktop seat the `uaccess` tag should cover
you; headless/SSH users must be in the `openterface` group and re-login. See
[permissions & udev](permissions-udev.md).

## `Device or resource busy` (serial)

Something else holds the port — often **ModemManager** probing a new ACM device,
or a leftover `openterface-rs`. Stop the other user, or tell ModemManager to
ignore the CH9329.

## Wrong video device is picked / black or garbled video

In a VM a **virtio-media `/dev/video0`** decoder adapter and metadata nodes
appear alongside the real capture. openterface-rs selects the **`uvcvideo` +
`MJPG`** node automatically. If detection still picks the wrong one, pass it
explicitly:

```bash
v4l2-ctl --list-devices
openterface-rs connect --video /dev/videoN
```

If the node has no `MJPG` format (`v4l2-ctl -d /dev/videoN --list-formats`), it
is not the Openterface capture.

## CH9329 `GET_INFO` returns nothing

Harmless. Some firmware does not answer `GET_INFO`; openterface-rs treats a
missing response as success and continues. It is **not** a connection failure.

## Input lags, repeats, or releases stick

The CH9329 over USB/IP drains absolute-move commands at only ~30–40/sec.
openterface-rs paces mouse moves to ~30 Hz by default. If you raised
`OPENTERFACE_MOUSE_INTERVAL_MS` below ~30 ms (faster than ~33 Hz) you can overrun
the chip and delay releases — restore the default (`33`). See
[environment variables](../reference/env-vars.md).

## Window has no decorations / won't resize (niri / tiling Wayland)

niri is CSD-only and advertises no server-side decorations; openterface-rs draws
client-side decorations via libdecor (winit). Ensure `libdecor` is installed at
runtime. `OPENTERFACE_FULLSCREEN=1` starts fullscreen.

## `connect` fails immediately with no window

A display session needs the `hardware` feature **and** a Wayland session
(`WAYLAND_DISPLAY` set) plus a Vulkan loader/driver. A binary built without
`hardware` can only `scan`/`status`. Over plain SSH there is no compositor — use
the [closed-loop harness](closed-loop-harness.md) to verify a headless device.

## Vulkan / wgpu: "no adapter found"

Install a Vulkan loader and driver (`libvulkan1` + Mesa). For a software adapter
(CI/headless) Mesa **lavapipe** works; point `VK_ICD_FILENAMES` at its ICD JSON.

## Raspberry Pi / aarch64

Use a **64-bit** OS; the prebuilt `aarch64-unknown-linux-gnu` binary is 64-bit
only. The same Wayland/Vulkan/udev/v4l runtime libraries are required.

## NixOS

Add the package's udev rules and the group:

```nix
services.udev.packages = [ pkgs.openterface-rs ];
users.groups.openterface = { };
users.users.<you>.extraGroups = [ "openterface" ];
```

## USB/IP and VMs

When the device is attached to a VM over USB/IP, the ~30 Hz pacing is especially
important (USB/IP adds latency). Attach via your VM's supported mechanism; inside
the guest the nodes appear as normal `/dev/ttyACM*` / `/dev/video*`.
