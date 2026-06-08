//! Headless wgpu render-pipeline test (only with `--features gpu-tests`).
//!
//! Validates the texture-upload + sampled-quad pipeline with no window by
//! rendering to an off-screen texture and reading the pixels back. Skips
//! gracefully (returns early, not a failure) when no GPU/software adapter is
//! available, so generic CI stays green.

#![cfg(feature = "gpu-tests")]

use openterface_core::decode::RgbaImage;

#[test]
fn renders_uploaded_frame_offscreen() {
    let instance = wgpu::Instance::default();
    let Some(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: true, // prefer a software adapter
        }))
    else {
        eprintln!("no wgpu adapter available; skipping headless render test");
        return;
    };
    let Ok((device, queue)) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
    else {
        eprintln!("no wgpu device; skipping");
        return;
    };

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = openterface_gui::renderer_for_test(device, queue, format);

    // A 2x2 solid red image.
    let img = RgbaImage {
        width: 2,
        height: 2,
        pixels: vec![
            255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
        ],
    };
    renderer.upload(&img);

    let pixels = renderer.render_to_buffer(4, 4);
    // The center of the target should be red (the texture is solid red).
    let idx = ((4 * 2) + 2) * 4; // pixel (2,2)
    assert!(
        pixels[idx] > 200,
        "expected red, got {:?}",
        &pixels[idx..idx + 4]
    );
    assert!(pixels[idx + 1] < 60);
    assert!(pixels[idx + 2] < 60);
}
