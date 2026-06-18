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
    let require = std::env::var("OPENTERFACE_REQUIRE_GPU").is_ok();
    // Enumerate adapters and prefer a software/CPU one (e.g. Mesa lavapipe,
    // which presents as a regular Vulkan adapter of device type Cpu rather than
    // a wgpu "fallback" adapter, so `force_fallback_adapter` alone misses it).
    let mut adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    adapters.sort_by_key(|a| match a.get_info().device_type {
        wgpu::DeviceType::Cpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::DiscreteGpu => 3,
        wgpu::DeviceType::Other => 4,
    });
    let adapter = adapters.into_iter().next().or_else(|| {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: true,
        }))
        .ok()
    });
    let Some(adapter) = adapter else {
        if require {
            panic!("OPENTERFACE_REQUIRE_GPU is set but no wgpu adapter was found");
        }
        eprintln!("no wgpu adapter available; skipping headless render test");
        return;
    };
    let device_queue =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()));
    let (device, queue) = match device_queue {
        Ok(dq) => dq,
        Err(e) => {
            if require {
                panic!("OPENTERFACE_REQUIRE_GPU is set but no wgpu device: {e}");
            }
            eprintln!("no wgpu device; skipping");
            return;
        }
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
