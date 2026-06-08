//! The winit (Wayland-only) application: window, wgpu surface, input capture,
//! and the render/throttle loop (behind `display`).
//!
//! This is the glue validated live on niri (W4.2) and work-ssd (W6). It honors
//! the `OPENTERFACE_*` tunables (mouse pacing via the core scheduler, idle
//! decode throttle, fullscreen, CSD) and keeps resize work off the input path.

use std::sync::mpsc::Receiver;
use std::time::Instant;

use openterface_core::decode::decode_frame;
use openterface_core::event::{InputEvent, Modifiers};
use openterface_core::protocol::hid::modifier_bit;
use openterface_core::session::Session;
use openterface_core::video::Frame;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::coord::window_to_abs;
use crate::input_map::{mouse_button, physical_to_hid};
use crate::throttle::{FrameThrottle, ThrottleConfig};

/// Configuration for [`run`].
pub struct RunConfig {
    /// The live session driving the target (input sink). `None` in dummy mode.
    pub session: Option<Session>,
    /// Captured frames from the session's capture thread. `None` in dummy mode
    /// (a static test pattern is shown instead).
    pub frames: Option<Receiver<Frame>>,
    /// Open the window fullscreen (`OPENTERFACE_FULLSCREEN`).
    pub fullscreen: bool,
    /// Window title.
    pub title: String,
}

/// Runs the display loop until the window is closed. Wayland-only.
pub fn run(config: RunConfig) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = App::new(config);
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct GpuWindow {
    window: std::sync::Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: crate::renderer::Renderer,
}

struct App {
    cfg: RunConfig,
    gpu: Option<GpuWindow>,
    throttle: FrameThrottle,
    /// The CH9329 modifier byte, tracked from physical modifier-key events so
    /// left/right modifiers (incl. AltGr) are preserved layout-independently.
    mod_byte: Modifiers,
    start: Instant,
}

impl App {
    fn new(cfg: RunConfig) -> Self {
        Self {
            cfg,
            gpu: None,
            throttle: FrameThrottle::new(ThrottleConfig::from_env()),
            mod_byte: Modifiers::NONE,
            start: Instant::now(),
        }
    }

    fn now(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    /// Forwards an input event to the session, if one is attached (no-op in
    /// dummy mode).
    fn send(&self, event: InputEvent) {
        if let Some(s) = &self.cfg.session {
            s.send_input(event);
        }
    }

    /// Releases all held input on the target (focus loss / pointer leave).
    fn release_all(&self) {
        if let Some(s) = &self.cfg.session {
            s.release_all();
        }
    }

    /// Pulls any available capture frames, applying the idle-decode throttle,
    /// and uploads the newest decoded image to the GPU.
    fn pump_frames(&mut self) {
        if self.gpu.is_none() {
            return;
        }
        let Some(frames) = self.cfg.frames.as_ref() else {
            return; // dummy mode: a static pattern was uploaded in `resumed`
        };
        let now = self.now();
        let mut newest: Option<Frame> = None;
        while let Ok(frame) = frames.try_recv() {
            newest = Some(frame);
        }
        let mut uploaded = false;
        if let Some(frame) = newest {
            if self.throttle.should_decode(now, &frame.data) {
                if let Ok(img) = decode_frame(&frame) {
                    if self.throttle.should_upload(now, &img.pixels) {
                        let gpu = self.gpu.as_mut().expect("checked above");
                        gpu.renderer.upload(&img);
                        gpu.window.request_redraw();
                        uploaded = true;
                    }
                }
            }
        }
        if !uploaded && self.throttle.should_force_refresh(now) {
            // Record the refresh so the watchdog fires once per interval rather
            // than spinning every loop under ControlFlow::Poll.
            self.throttle.note_refresh(now);
            if let Some(gpu) = self.gpu.as_ref() {
                gpu.window.request_redraw();
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes().with_title(self.cfg.title.clone());
        if self.cfg.fullscreen {
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        let window = std::sync::Arc::new(el.create_window(attrs).expect("create window"));

        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("request adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .expect("request device");

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let mut renderer = crate::renderer::Renderer::new(device, queue, format);

        // Dummy mode (no capture): upload a static test pattern so the window is
        // not blank and the render path is exercised without a device.
        if self.cfg.frames.is_none() {
            renderer.upload(&test_pattern(640, 360));
        }

        self.gpu = Some(GpuWindow {
            window,
            surface,
            config,
            renderer,
        });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                // Release everything before tearing down the session so the
                // target never sees input stuck down after the window closes.
                self.mod_byte = Modifiers::NONE;
                self.release_all();
                el.exit();
            }
            WindowEvent::Resized(size) => {
                // Resize the GPU surface here (render thread), never on the input
                // path, so input dispatch is never blocked.
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(gpu.renderer.device(), &gpu.config);
                }
            }
            WindowEvent::Focused(false) => {
                // Releasing focus: drop all held keys/buttons so the target
                // never sees stuck input (C++ focus-loss parity).
                self.mod_byte = Modifiers::NONE;
                self.release_all();
            }
            WindowEvent::CursorLeft { .. } => {
                self.mod_byte = Modifiers::NONE;
                self.release_all();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(usage) = physical_to_hid(event.physical_key) {
                    let pressed = event.state == ElementState::Pressed;
                    // Track the CH9329 modifier byte from physical modifier keys
                    // (preserves left/right and AltGr, layout-independently).
                    if let Some(bit) = modifier_bit(usage) {
                        self.mod_byte = if pressed {
                            self.mod_byte.union(bit)
                        } else {
                            Modifiers(self.mod_byte.0 & !bit.0)
                        };
                    }
                    self.throttle.note_input(self.now());
                    self.send(InputEvent::Key {
                        usage,
                        modifiers: self.mod_byte,
                        pressed,
                    });
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(gpu) = self.gpu.as_ref() {
                    let pos =
                        window_to_abs(position.x, position.y, gpu.config.width, gpu.config.height);
                    self.throttle.note_input(self.now());
                    self.send(InputEvent::MouseMoveAbsolute { pos });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(b) = mouse_button(button) {
                    self.throttle.note_input(self.now());
                    self.send(InputEvent::MouseButton {
                        button: b,
                        pressed: state == ElementState::Pressed,
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let ticks = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as i32,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0).round() as i32,
                };
                if ticks != 0 {
                    self.throttle.note_input(self.now());
                    self.send(InputEvent::Scroll {
                        delta: ticks.clamp(-127, 127) as i8,
                    });
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(gpu) = self.gpu.as_mut() {
                    if let Ok(surface_tex) = gpu.surface.get_current_texture() {
                        let view = surface_tex
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder = gpu.renderer.device().create_command_encoder(
                            &wgpu::CommandEncoderDescriptor {
                                label: Some("frame"),
                            },
                        );
                        gpu.renderer.draw(&view, &mut encoder);
                        gpu.renderer.queue().submit(Some(encoder.finish()));
                        surface_tex.present();
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.pump_frames();
        // Poll the capture channel at ~250 Hz rather than spinning the event
        // loop continuously (a frame at 30 fps arrives every ~33 ms; 4 ms keeps
        // latency low without burning a core under ControlFlow::Poll).
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + std::time::Duration::from_millis(4),
        ));
    }
}

/// Builds a simple RGBA gradient test pattern for dummy mode.
fn test_pattern(width: u32, height: u32) -> openterface_core::decode::RgbaImage {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255 / width.max(1)) as u8;
            let g = (y * 255 / height.max(1)) as u8;
            pixels.extend_from_slice(&[r, g, 0x40, 0xFF]);
        }
    }
    openterface_core::decode::RgbaImage {
        width,
        height,
        pixels,
    }
}
