# W1.1 — Wayland input go/no-go on niri

**Decision: GO with `winit` (+ `wgpu`).** No need for `smithay-client-toolkit`,
`libei`, or a compositor-specific path. niri advertises every Wayland protocol
the KVM input model needs, and a `winit` smoke test creates a decorated window
and acquires pointer locks on the live niri session.

This is the project's highest-risk architectural question (the plan flagged it
as a go/no-go gate before committing the display stack). It is now resolved.

## Why it matters

A KVM frontend must:
1. **Forward keyboard** to the target while focused — without the host acting on
   those keys (Wayland routes input only to the focused surface, modulo
   compositor-reserved shortcuts; this is the desired behavior).
2. **Absolute mouse mode** — map window-relative pointer coordinates into the
   CH9329 `0..=4095` space (`wl_pointer` motion).
3. **Relative mouse mode** — lock/hide the cursor and read *raw* relative
   motion. On Wayland this requires `zwp_pointer_constraints_v1` (locked/confined
   pointer) + `zwp_relative_pointer_manager_v1`.
4. **Draw its own decorations** — niri is CSD-only and advertises no
   `zxdg_decoration_manager_v1`.

If niri lacked pointer-constraints/relative-pointer, relative mouse mode would be
impossible with plain `winit` and we'd have needed a different stack. It does
not lack them.

## Evidence

### 1. niri's advertised Wayland globals

`wayland-info` against the live niri session (`WAYLAND_DISPLAY=wayland-1`):

```
interface: 'wl_compositor',                   version: 6,  name: 1
interface: 'xdg_wm_base',                      version: 7,  name: 3
interface: 'zwp_tablet_manager_v2',            version: 1,  name: 11
interface: 'zwp_relative_pointer_manager_v1',  version: 1,  name: 13
interface: 'zwp_pointer_constraints_v1',       version: 1,  name: 14
interface: 'zwp_virtual_keyboard_manager_v1',  version: 1,  name: 26
interface: 'zwlr_virtual_pointer_manager_v1',  version: 2,  name: 27
interface: 'wl_seat',                          version: 9,  name: 40
```

- `zwp_relative_pointer_manager_v1` + `zwp_pointer_constraints_v1` → **relative
  mouse mode is supported.**
- **No `zxdg_decoration_manager_v1`** → confirms niri is **CSD-only**; the
  frontend must draw client-side decorations.
- Bonus: `zwp_virtual_keyboard_manager_v1` / `zwlr_virtual_pointer_manager_v1`
  exist (not needed by us, but available).

### 2. `winit` smoke test on the live niri session

A minimal `winit 0.30` app (no wgpu) run against niri:

```
[spike] OK window created id=WindowId(...)
[spike] is_decorated=true
[spike] set_cursor_grab(Locked) = OK
[spike] set_cursor_grab(Confined) = OK
[spike] done (3s)
```

- `winit` connects to niri's Wayland backend and **creates a window**.
- `is_decorated=true` → `winit` provides **client-side decorations**
  (`sctk-adwaita`) without a server decoration manager.
- **Both `CursorGrabMode::Locked` and `Confined` succeed** → `winit` wires up
  `zwp_pointer_constraints_v1`; relative mouse mode is achievable via
  `winit`'s `DeviceEvent::MouseMotion` (raw relative motion) under a lock.

## What this fixes for the display stack (W4.2)

- Use **`winit` (Wayland-only, X11 feature disabled) + `wgpu`**.
- Enable `winit`'s Wayland **CSD** (`sctk-adwaita`/libdecor-equivalent); honor an
  `OPENTERFACE_USE_LIBDECOR=0` escape to a bare window.
- **Absolute mode:** `WindowEvent::CursorMoved` → map to `0..=4095`.
- **Relative mode:** `set_cursor_grab(Locked)` + hide cursor + consume
  `DeviceEvent::MouseMotion`; long-press-Esc exits.
- Resize off the input thread; content-area clicks always forward.

## Packaging note (W4.2 / W6)

`winit` (and `wgpu`) **`dlopen` their system libraries at runtime**. On NixOS the
spike failed with `WaylandError(Connection(NoWaylandLib))` until
`libwayland-client.so` and `libxkbcommon.so` were on `LD_LIBRARY_PATH`. The
`openterface-gui` runtime therefore needs, on its library path:
`libwayland-client`, `libxkbcommon`, and (for `wgpu`) `vulkan-loader` + `libGL`.
The repo `flake.nix` dev shell already sets this; the W6 Nix package derivation
must wrap the binary with the same runtime libraries.

## Not verified non-interactively (low risk)

- Actual key/motion *event delivery* needs window focus + input; not scripted
  here. Low risk: it is standard `winit`/Wayland behavior and is exercised on
  niri in the work-ssd VM.
- "Host does not also receive forwarded keys" is guaranteed by Wayland's
  focus-routing model (only the focused surface receives input).
