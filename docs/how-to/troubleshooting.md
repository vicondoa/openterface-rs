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

## Paste does nothing or only partially types

- The openterface-rs window must be focused. Paste uses Wayland's normal
  focused-client clipboard path, not a global clipboard scrape.
- Paste requires input forwarding. It will not type when `--no-serial` is set or
  when the serial device failed to open/negotiate.
- `OPENTERFACE_ENABLE_PASTE=0` disables the host-local `Ctrl+Shift+V`
  shortcut; when disabled, the combo is forwarded like ordinary input.
- `OPENTERFACE_PASTE_SHORTCUT=ctrl-alt-shift-v` (or another modifier+`v` chord)
  changes the host-local shortcut if `Ctrl+Shift+V` conflicts with your target
  workflow.
- Keyboard paste uses the regular clipboard. Middle-click is forwarded to the
  target by default; set `OPENTERFACE_MIDDLE_CLICK_PASTE=primary` to use the host
  primary selection, or `clipboard` to use the regular clipboard.
- Empty clipboards or clipboards without text are reported as paste warnings and
  do not type anything.
- The first implementation types US-layout ASCII only. Unsupported characters
  (for example accents, smart quotes, emoji, or `€`) are skipped and reported by
  count. Newline becomes Enter and tab becomes Tab.
- Very large clipboards are capped by `OPENTERFACE_PASTE_MAX_CHARS`; truncated
  text is reported in the window title while queued paste frames drain. Press
  Escape, unfocus, or close the window to abort queued paste frames.
- If your compositor or fullscreen mode hides window titles, paste feedback is
  still available in logs but may not be visible in the window itself.

Clipboard contents are never logged, even with verbose tracing; only counts and
static error categories are logged.

## Window has no decorations / won't resize (niri / tiling Wayland)

By default openterface-rs opens an **undecorated** xdg-shell window (no
client-side title bar). This is deliberate: winit's client-side decorations
(CSD) commit the toplevel out of band from wgpu's surface presentation and race
on focus/visibility changes, which can make the window disappear (process still
running) on CSD-only compositors like niri. On a tiling compositor a client
title bar is redundant anyway — niri shows the title/status itself. Set
`OPENTERFACE_USE_LIBDECOR=1` to draw a client-side title bar (useful on a
floating compositor where you want mouse-driven move/resize/close controls).
`OPENTERFACE_FULLSCREEN=1` starts fullscreen.

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
