# W1.3 — wgpu render + headless-CI spike

**Status: approach fixed; software-adapter render test is feature-gated and
skips cleanly when no adapter is present.** The on-screen present path is
validated for real in W4.2 (on niri) and W6 (work-ssd). This note keeps the
heavy `wgpu` dependency out of the way of the no-hardware CI guarantee.

## Render design (W4.2)

The frontend renders a single decoded frame per refresh:

1. `decode::decode_frame` → `RgbaImage` (W2.3).
2. Upload to a `wgpu::Texture` (`R8G8B8A8Unorm`), `write_texture` on change only
   (idle-decode throttle skips unchanged frames — see `cpp-cli-behavior.md`).
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
