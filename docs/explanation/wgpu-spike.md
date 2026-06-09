# W1.3 — wgpu render + headless-CI spike

**Status: approach fixed; software-adapter render test is feature-gated and
skips cleanly when no adapter is present.** The on-screen present path is
validated for real in W4.2 (on niri) and W6 (work-ssd). This note keeps the
heavy `wgpu` dependency out of the way of the no-hardware CI guarantee.

## Idle-decode throttle state machine (W4.2 design)

"Upload on change" alone does not throttle the expensive *decode*. The GUI uses
a small state machine:

1. **Raw dedup:** hash/byte-compare the *encoded* MJPEG payload against the last
   frame; if identical, skip decode **and** upload entirely.
2. **Non-deterministic MJPEG:** some encoders emit byte-different frames for an
   unchanged screen. After the raw compare fails, decode and **compare decoded
   pixels** to the last decoded frame; if equal, skip the upload.
3. **Idle declaration:** after ~15 consecutive static frames (~0.5 s @ 30 fps),
   enter idle and cap decode attempts to `OPENTERFACE_IDLE_DECODE_MS` (default
   100 ms).
4. **Input wake:** any forwarded input keeps full-rate decode for
   `OPENTERFACE_INPUT_WAKE_MS` (default 250 ms).
5. **Anti-freeze watchdog:** force a surface refresh of the cached frame at least
   every `OPENTERFACE_IDLE_WATCHDOG_MS` (default 1000 ms), no decode required.
6. **`OPENTERFACE_THROTTLE=0`** disables all of the above (always decode).

This lives in `openterface-gui` (orchestration) using `decode` + `wgpu`; the
state machine is unit-testable with a fake clock and scripted frame sequences.

## CI: a mandatory software-adapter render job (W4.2)

The feature-gated test self-skips when no adapter is present, which keeps
generic CI hardware-free — but the render path must actually run *somewhere*.
W4.2 adds **one CI job (and a devshell path) with a software Vulkan backend**
(Mesa **lavapipe**, `VK_ICD_FILENAMES` for `lvp`) where the gated test is
required, so texture upload + offscreen pixel assertions are genuinely
exercised. Other jobs keep the skip behavior.

## Render design (W4.2)

The frontend renders a single decoded frame per refresh:

1. `decode::decode_frame` → `RgbaImage` (W2.3).
2. Upload to a `wgpu::Texture` (`R8G8B8A8Unorm`), `write_texture` on change only
   (idle-decode throttle skips unchanged frames).
3. Draw a full-screen triangle/quad sampling the texture; present to the
   `winit` surface.

`wgpu` is confined to the **`openterface-gui`** crate so `openterface-core` and
`openterface-cli` tests never pay its (large) compile cost.

## Headless test strategy (the no-hardware guarantee)

- A render-pipeline test runs **without a window**: create a `wgpu::Device` from
  any adapter (incl. a software one), render the textured quad to an off-screen
  texture, `copy_texture_to_buffer`, map and **assert pixel values**.
- The test is **feature-gated** (e.g. `--features gpu-tests`) and, when run,
  **skips gracefully** (returns early, not a failure) if
  `request_adapter` yields `None`. This prevents flakiness on CI runners with no
  GPU/loader.
- **CI:** to actually exercise the path, install a software Vulkan stack
  (Mesa **lavapipe** / `VK_ICD_FILENAMES` for `lvp`), or the GL backend; if the
  runner has neither, the gated test self-skips and the rest of CI stays green.

## NixOS / packaging note

`wgpu` `dlopen`s the **Vulkan loader** (`libvulkan.so`) and may use `libGL`. As
with `winit` (see `wayland-input-spike.md`), the runtime needs `vulkan-loader`
(+ an ICD) and `libGL` on its library path. The `flake.nix` dev shell sets this;
the W6 Nix package derivation must wrap the binary accordingly. On the work-ssd
VM the existing VAAPI/virtio-gpu stack provides the GPU path.

## Deferred / verified later

- Real adapter selection + present on niri (W4.2) and in work-ssd (W6).
- Color/range correctness of the YUYV→RGBA→texture path (W2.3 + W4.2).
- Whether to prefer hardware MJPEG decode (VAAPI) over CPU `zune-jpeg` on the
  Pi/VM — measured in W4.2; CPU decode is the baseline.
