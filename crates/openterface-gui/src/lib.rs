//! `openterface-gui` — the native Wayland display frontend.
//!
//! This crate isolates the heavy `winit` + `wgpu` dependencies from the core
//! and CLI so their tests build fast. It renders decoded frames
//! ([`openterface_core::decode::RgbaImage`]) and captures window input,
//! forwarding it through the core pipeline.
//!
//! Implemented in **W4.2** (gated on the W1.1 Wayland-input go/no-go on niri).
//! W0 leaves it as a documented placeholder.

// (winit + wgpu render and window input capture land in W4.2.)
