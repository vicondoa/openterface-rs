//! `openterface-gui` — the native Wayland display frontend.
//!
//! This crate isolates the heavy `winit` + `wgpu` dependencies behind the
//! `display` feature so the rest of the workspace builds and tests with no
//! windowing/GPU system libraries. The **pure** pieces — window→target
//! coordinate mapping ([`coord`]) and the idle-decode throttle state machine
//! ([`throttle`]) — are always built and unit-tested.
//!
//! With `--features display`, [`run`] opens a winit (Wayland-only) window, drives
//! a `wgpu` renderer of decoded frames, and captures window input, forwarding it
//! through an [`openterface_core::session::Session`]. The display path is
//! validated live on niri (W4.2) and on the work-ssd VM (W6).

pub mod coord;
pub mod throttle;

#[cfg(feature = "display")]
mod app;
#[cfg(feature = "display")]
mod input_map;
#[cfg(feature = "display")]
pub mod renderer;

#[cfg(feature = "display")]
pub use app::{run, RunConfig};

/// Test-only constructor for the renderer (used by the headless render test).
#[cfg(feature = "display")]
pub fn renderer_for_test(
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
) -> renderer::Renderer {
    renderer::Renderer::new(device, queue, format)
}
