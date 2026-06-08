//! The winit (Wayland-only) application: window, wgpu surface, input capture,
//! and the render/throttle loop (behind `display`).
//!
//! This is the glue validated live on niri (W4.2) and work-ssd (W6). It honors
//! the `OPENTERFACE_*` tunables (mouse pacing via the core scheduler, idle
//! decode throttle, fullscreen, CSD) and keeps resize work off the input path.

use std::sync::mpsc::Receiver;
use std::time::Instant;

use openterface_core::decode::decode_frame;
use openterface_core::event::InputEvent;
use openterface_core::session::Session;
use openterface_core::video::Frame;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::coord::window_to_abs;
use crate::input_map::{modifiers_from_winit, mouse_button, physical_to_hid};
use crate::throttle::{FrameThrottle, ThrottleConfig};

/// Configuration for [`run`].
pub struct RunConfig {
    /// The live session driving the target (input sink).
    pub session: Session,
    /// Captured frames from the session's capture thread.
    pub frames: Receiver<Frame>,
    /// Open the window fullscreen (`OPENTERFACE_FULLSCREEN`).
    pub fullscreen: bool,
    /// Window title.
    pub title: String,
}

/// Runs the display loop until the window is closed. Wayland-only.
pub fn run(config: RunConfig) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
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
    modifiers: ModifiersState,
    start: Instant,
}

impl App {
    fn new(cfg: RunConfig) -> Self {
        Self {
            cfg,
            gpu: None,
            throttle: FrameThrottle::new(ThrottleConfig::from_env()),
            modifiers: ModifiersState::empty(),
            start: Instant::now(),
        }
    }

    fn now(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    /// Pulls any available capture frames, applying the idle-decode throttle,
    /// and uploads the newest decoded image to the GPU.
    fn pump_frames(&mut self) {
        if self.gpu.is_none() {
            return;
        }
        let now = self.now();
        let mut newest: Option<Frame> = None;
        while let Ok(frame) = self.cfg.frames.try_recv() {
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
        let renderer = crate::renderer::Renderer::new(device, queue, format);

        self.gpu = Some(GpuWindow {
            window,
            surface,
            config,
            renderer,
        });
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                // Resize the GPU surface here (render thread), never on the input
                // path, so input dispatch is never blocked.
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(gpu.renderer.device(), &gpu.config);
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(usage) = physical_to_hid(event.physical_key) {
                    self.throttle.note_input(self.now());
                    self.cfg.session.send_input(InputEvent::Key {
                        usage,
                        modifiers: modifiers_from_winit(self.modifiers),
                        pressed: event.state == ElementState::Pressed,
                    });
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(gpu) = self.gpu.as_ref() {
                    let pos =
                        window_to_abs(position.x, position.y, gpu.config.width, gpu.config.height);
                    self.throttle.note_input(self.now());
                    self.cfg
                        .session
                        .send_input(InputEvent::MouseMoveAbsolute { pos });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(b) = mouse_button(button) {
                    self.throttle.note_input(self.now());
                    self.cfg.session.send_input(InputEvent::MouseButton {
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
                    self.cfg.session.send_input(InputEvent::Scroll {
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

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        self.pump_frames();
    }
}
