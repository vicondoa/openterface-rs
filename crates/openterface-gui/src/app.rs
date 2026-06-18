//! The winit (Wayland-only) application: window, wgpu surface, input capture,
//! and the render/throttle loop (behind `display`).
//!
//! This is the glue validated live on niri (W4.2) and work-ssd (W6). It honors
//! the `OPENTERFACE_*` tunables (mouse pacing via the core scheduler, idle
//! decode throttle, fullscreen, CSD) and keeps resize work off the input path.

use std::sync::mpsc::Receiver;
use std::time::Duration;
use std::time::Instant;

use openterface_core::decode::decode_frame;
use openterface_core::event::{HidUsage, InputEvent, Modifiers, MouseButton};
use openterface_core::pacing::DEFAULT_COMMAND_GAP;
use openterface_core::paste::PasteOutcome;
use openterface_core::protocol::hid::modifier_bit;
use openterface_core::session::Session;
use openterface_core::video::Frame;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::{Window, WindowId};
use zeroize::Zeroize;

use crate::clipboard::{ClipboardError, ClipboardReader};
use crate::coord::{window_to_abs, window_to_abs_fit};
use crate::input_map::{mouse_button, physical_to_hid};
use crate::paste::{
    chord_usages, is_paste_hotkey, MiddleClickPaste, PasteConfig, PasteSource, SuppressedKeys,
};
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
    /// Log each forwarded input event (`connect --debug`).
    pub debug: bool,
    /// Whether serial input forwarding is backed by a real CH9329 transport.
    pub input_available: bool,
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
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: crate::renderer::Renderer,
}

struct TitleStatus {
    message: String,
    until: Option<Instant>,
}

struct App {
    cfg: RunConfig,
    base_title: String,
    gpu: Option<GpuWindow>,
    throttle: FrameThrottle,
    paste: PasteConfig,
    suppressed_keys: SuppressedKeys,
    clipboard: Option<ClipboardReader>,
    paste_active_until: Option<Instant>,
    title_status: Option<TitleStatus>,
    suppress_middle_release: bool,
    /// The CH9329 modifier byte, tracked from physical modifier-key events so
    /// left/right modifiers (incl. AltGr) are preserved layout-independently.
    mod_byte: Modifiers,
    start: Instant,
    /// A window resize whose GPU surface reconfigure is deferred to the next
    /// redraw, off the event-dispatch path.
    pending_resize: Option<(u32, u32)>,
    frames_seen: u64,
    uploads: u64,
    decode_errors: u64,
    /// Native size of the most recently displayed frame, used to map the pointer
    /// through the same letterbox/pillarbox fit the renderer applies. `None`
    /// until the first frame is shown (no video on screen yet).
    frame_size: Option<(u32, u32)>,
}

impl App {
    fn new(cfg: RunConfig) -> Self {
        let base_title = cfg.title.clone();
        Self {
            cfg,
            base_title,
            gpu: None,
            throttle: FrameThrottle::new(ThrottleConfig::from_env()),
            paste: PasteConfig::from_env(),
            suppressed_keys: SuppressedKeys::default(),
            clipboard: None,
            paste_active_until: None,
            title_status: None,
            suppress_middle_release: false,
            mod_byte: Modifiers::NONE,
            start: Instant::now(),
            pending_resize: None,
            frames_seen: 0,
            uploads: 0,
            decode_errors: 0,
            frame_size: None,
        }
    }

    fn now(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    /// Forwards an input event to the session, if one is attached (no-op in
    /// dummy mode).
    fn send(&self, event: InputEvent) {
        if self.cfg.debug {
            // Per SECURITY.md, never log key identity/typed text: redact
            // keyboard events to just the press/release edge. Mouse
            // motion/buttons/scroll are not sensitive.
            match &event {
                InputEvent::Key { pressed, .. } | InputEvent::PasteKey { pressed, .. } => {
                    tracing::info!(pressed = *pressed, "key event (redacted)")
                }
                other => tracing::info!(event = ?other, "input"),
            }
        }
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

    fn set_status(&mut self, message: impl Into<String>, linger: bool) {
        let until = if linger {
            None
        } else {
            Some(Instant::now() + Duration::from_secs(3))
        };
        self.set_status_until(message, until);
    }

    fn set_status_until(&mut self, message: impl Into<String>, until: Option<Instant>) {
        self.title_status = Some(TitleStatus {
            message: message.into(),
            until,
        });
        self.update_title();
    }

    fn clear_expired_status(&mut self) {
        let expired = self
            .title_status
            .as_ref()
            .and_then(|status| status.until)
            .is_some_and(|until| Instant::now() >= until);
        if expired {
            self.title_status = None;
            self.update_title();
        }
        if self
            .paste_active_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.paste_active_until = None;
        }
    }

    fn update_title(&self) {
        if let Some(gpu) = self.gpu.as_ref() {
            match self.title_status.as_ref() {
                Some(status) => gpu
                    .window
                    .set_title(&format!("{} — {}", self.base_title, status.message)),
                None => gpu.window.set_title(&self.base_title),
            }
        }
    }

    fn abort_paste(&mut self, message: &'static str) -> bool {
        let had_active = self.paste_active_until.take().is_some();
        if had_active {
            self.mod_byte = Modifiers::NONE;
            self.release_all();
            self.set_status(message, true);
            tracing::warn!(category = "aborted", "paste aborted");
            true
        } else {
            false
        }
    }

    fn start_paste(&mut self, source: PasteSource) {
        self.mod_byte = Modifiers::NONE;
        self.release_all();

        if self.cfg.session.is_none() {
            self.set_status("Paste warning: no active session", true);
            tracing::warn!(category = "no-session", "paste rejected");
            return;
        }
        if !self.cfg.input_available {
            self.set_status("Paste warning: input forwarding unavailable", true);
            tracing::warn!(category = "no-input", "paste rejected");
            return;
        }
        let Some(reader) = self.clipboard.clone() else {
            self.set_status("Paste warning: Wayland clipboard unavailable", true);
            tracing::warn!(category = "clipboard-unavailable", "paste rejected");
            return;
        };

        self.set_status(format!("Paste: reading {}", source.label()), false);
        let result = match source {
            PasteSource::Clipboard => reader.load_regular(),
            PasteSource::Primary => reader.load_primary(),
        };
        self.handle_clipboard_result(result);
    }

    fn handle_clipboard_result(&mut self, result: Result<String, ClipboardError>) {
        let mut text = match result {
            Ok(text) => text,
            Err(e) => {
                self.set_status(format!("Paste warning: clipboard {}", e.category()), true);
                tracing::warn!(category = e.category(), "paste failed");
                return;
            }
        };

        self.mod_byte = Modifiers::NONE;
        self.release_all();
        let outcome = self
            .cfg
            .session
            .as_ref()
            .map(|session| session.send_paste(&text, self.paste.max_chars))
            .unwrap_or_default();
        text.zeroize();

        self.report_paste_outcome(outcome);
    }

    fn report_paste_outcome(&mut self, outcome: PasteOutcome) {
        let stats = outcome.stats;
        if outcome.reports > 0 {
            self.paste_active_until = Some(Instant::now() + estimated_paste_duration(outcome));
        }
        if stats.submitted > 0 && (stats.truncated > 0 || stats.skipped > 0) {
            self.set_status_until(
                format!(
                    "Pasting {} chars... skipped {}, truncated {} — Esc aborts",
                    stats.submitted, stats.skipped, stats.truncated
                ),
                self.paste_active_until,
            );
            tracing::warn!(
                submitted = stats.submitted,
                skipped = stats.skipped,
                truncated = stats.truncated,
                "paste completed with warnings"
            );
        } else if stats.truncated > 0 || stats.skipped > 0 {
            self.set_status(
                format!(
                    "Paste warning: no mappable text, skipped {}, truncated {}",
                    stats.skipped, stats.truncated
                ),
                true,
            );
            tracing::warn!(
                submitted = stats.submitted,
                skipped = stats.skipped,
                truncated = stats.truncated,
                "paste completed with warnings"
            );
        } else if stats.submitted == 0 {
            self.set_status("Paste warning: no mappable text", true);
            tracing::warn!(
                category = "no-mappable-text",
                "paste completed with warnings"
            );
        } else {
            self.set_status_until(
                format!("Pasting {} chars... Esc aborts", stats.submitted),
                self.paste_active_until,
            );
            tracing::info!(submitted = stats.submitted, "paste submitted");
        }
    }

    fn teardown_clipboard(&mut self) {
        self.clipboard = None;
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
            self.frames_seen += 1;
            if self.frames_seen == 1 {
                tracing::debug!(bytes = frame.data.len(), "first frame reached GUI");
            }
            if self.throttle.should_decode(now, &frame.data) {
                match decode_frame(&frame) {
                    Ok(img) => {
                        if self.throttle.should_upload(now, &img.pixels) {
                            let gpu = self.gpu.as_mut().expect("checked above");
                            gpu.renderer.upload(&img);
                            self.frame_size = Some((img.width, img.height));
                            gpu.window.request_redraw();
                            uploaded = true;
                            if self.uploads == 0 {
                                tracing::debug!(
                                    w = img.width,
                                    h = img.height,
                                    "first frame uploaded to GPU"
                                );
                            }
                            self.uploads += 1;
                        }
                    }
                    Err(e) => {
                        self.decode_errors += 1;
                        if self.decode_errors == 1 || self.decode_errors.is_multiple_of(120) {
                            tracing::warn!(errors = self.decode_errors, error = %e, "decode failed");
                        }
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

    /// Presents the current frame, recovering from a stale surface.
    ///
    /// On Wayland the surface can go stale after the
    /// compositor re-shows the window (e.g. returning to it after a niri
    /// workspace/window switch), often with a changed size or scale. The earlier
    /// code dropped every `get_current_texture` error silently and never
    /// recovered, so the window stopped presenting and appeared frozen or gone
    /// with nothing logged. Reconfigure `Outdated`; recreate the surface on
    /// `Lost`; skip transient statuses.
    fn render(&mut self) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let surface_tex = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(tex) => tex,
            wgpu::CurrentSurfaceTexture::Suboptimal(tex) => {
                tracing::trace!("surface frame is suboptimal; presenting and rearming redraw");
                gpu.window.request_redraw();
                tex
            }
            status => {
                match surface_recovery(&status) {
                    SurfaceRecovery::Reconfigure => {
                        tracing::debug!("surface outdated; reconfiguring");
                        // Re-read the size: the surface is usually outdated
                        // because the compositor resized/rescaled us on re-show,
                        // so reconfiguring with the stale config would just go
                        // `Outdated` again (a permanent freeze / hot loop).
                        let size = gpu.window.inner_size();
                        gpu.config.width = size.width.max(1);
                        gpu.config.height = size.height.max(1);
                        gpu.surface.configure(gpu.renderer.device(), &gpu.config);
                        gpu.window.request_redraw();
                    }
                    SurfaceRecovery::Recreate => {
                        tracing::debug!("surface lost; recreating");
                        match gpu.instance.create_surface(gpu.window.clone()) {
                            Ok(surface) => {
                                let size = gpu.window.inner_size();
                                gpu.config.width = size.width.max(1);
                                gpu.config.height = size.height.max(1);
                                surface.configure(gpu.renderer.device(), &gpu.config);
                                gpu.surface = surface;
                                gpu.window.request_redraw();
                            }
                            Err(error) => {
                                tracing::warn!(?error, "failed to recreate lost surface");
                            }
                        }
                    }
                    SurfaceRecovery::Skip => {
                        tracing::trace!(?status, "surface frame skipped");
                    }
                }
                return;
            }
        };
        let view = surface_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            gpu.renderer
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame"),
                });
        gpu.renderer
            .draw(&view, &mut encoder, gpu.config.width, gpu.config.height);
        gpu.renderer.queue().submit(Some(encoder.finish()));
        surface_tex.present();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title(self.cfg.title.clone())
            // Wayland app-id (window rules / taskbar grouping).
            .with_name("openterface-rs", "openterface-rs")
            // Open at a 16:9 size so the window starts at the capture's shape
            // (the frame is letterboxed if the user later resizes off-ratio).
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            // Minimum sensible window size.
            .with_min_inner_size(LogicalSize::new(640.0, 480.0));
        // Window decorations. On a CSD-only compositor (niri) winit's decorated
        // path draws the title bar itself (SCTK client-side decorations) using
        // its own decoration subsurfaces, and commits the toplevel out of band
        // from wgpu's surface presentation. On a focus/visibility change — e.g.
        // returning to the window after a niri workspace switch — the CSD
        // configure/commit races wgpu's independent presents and the compositor
        // unmaps the toplevel: the window vanishes while the process keeps
        // rendering. Default to an undecorated xdg-shell window so there is a
        // single committer for the surface (the title/status is still shown by
        // the compositor via xdg_toplevel.title). Set OPENTERFACE_USE_LIBDECOR=1
        // to opt back into client-side decorations.
        let use_libdecor = decorations_enabled(std::env::var("OPENTERFACE_USE_LIBDECOR").ok());
        attrs = attrs.with_decorations(use_libdecor);
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
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
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
            self.frame_size = Some((640, 360));
        }

        self.clipboard = match ClipboardReader::from_window(&window) {
            Ok(reader) => Some(reader),
            Err(e) => {
                tracing::warn!(
                    category = e.category(),
                    "focused Wayland clipboard unavailable"
                );
                None
            }
        };
        tracing::info!(
            enabled = self.paste.enabled,
            max_chars = self.paste.max_chars,
            shortcut = self.paste.shortcut.label(),
            "focused paste shortcut"
        );

        self.gpu = Some(GpuWindow {
            window,
            instance,
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
                let _ = self.abort_paste("Paste warning: aborted");
                self.mod_byte = Modifiers::NONE;
                self.release_all();
                el.exit();
            }
            WindowEvent::Resized(size) => {
                // Defer the GPU surface reconfigure to the next redraw, off the
                // event-dispatch path, so a burst of resize events coalesces and
                // input is never blocked by GPU work.
                self.pending_resize = Some((size.width.max(1), size.height.max(1)));
                if let Some(gpu) = self.gpu.as_ref() {
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::Focused(focused) => {
                if focused {
                    // Regained focus (e.g. returning to the window after a niri
                    // workspace/window switch): re-arm a redraw so the surface is
                    // re-acquired and reconfigured if the compositor invalidated
                    // it while we were away, instead of staying on a stale frame.
                    if let Some(gpu) = self.gpu.as_ref() {
                        gpu.window.request_redraw();
                    }
                } else {
                    // Releasing focus: drop all held keys/buttons so the target
                    // never sees stuck input.
                    let _ = self.abort_paste("Paste warning: focus lost");
                    self.mod_byte = Modifiers::NONE;
                    self.release_all();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.mod_byte = Modifiers::NONE;
                self.release_all();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(usage) = physical_to_hid(event.physical_key) {
                    let pressed = event.state == ElementState::Pressed;
                    if self.suppressed_keys.suppress_event(usage, pressed) {
                        return;
                    }
                    if pressed
                        && usage == HidUsage(0x29)
                        && self.abort_paste("Paste warning: aborted")
                    {
                        return;
                    }
                    if self.paste.enabled
                        && pressed
                        && !event.repeat
                        && is_paste_hotkey(usage, self.mod_byte, self.paste.shortcut)
                    {
                        self.suppressed_keys
                            .extend(chord_usages(usage, self.mod_byte));
                        self.start_paste(PasteSource::Clipboard);
                        return;
                    }
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
                    // Map through the same contain-fit the renderer uses so the
                    // pointer lands on the right target pixel and the black bars
                    // are excluded. Before the first frame, fall back to the full
                    // surface (nothing is displayed yet anyway).
                    let pos = match self.frame_size {
                        Some((tw, th)) => window_to_abs_fit(
                            position.x,
                            position.y,
                            tw,
                            th,
                            gpu.config.width,
                            gpu.config.height,
                        ),
                        None => window_to_abs(
                            position.x,
                            position.y,
                            gpu.config.width,
                            gpu.config.height,
                        ),
                    };
                    self.throttle.note_input(self.now());
                    self.send(InputEvent::MouseMoveAbsolute { pos });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(b) = mouse_button(button) {
                    if b == MouseButton::Middle {
                        if state == ElementState::Released && self.suppress_middle_release {
                            self.suppress_middle_release = false;
                            return;
                        }
                        if self.paste.enabled && state == ElementState::Pressed {
                            match self.paste.middle_click {
                                MiddleClickPaste::Off => {}
                                MiddleClickPaste::Primary => {
                                    self.suppress_middle_release = true;
                                    self.start_paste(PasteSource::Primary);
                                    return;
                                }
                                MiddleClickPaste::Clipboard => {
                                    self.suppress_middle_release = true;
                                    self.start_paste(PasteSource::Clipboard);
                                    return;
                                }
                            }
                        }
                    }
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
                // Apply any deferred resize before drawing (off the input path).
                if let Some((w, h)) = self.pending_resize.take() {
                    if let Some(gpu) = self.gpu.as_mut() {
                        gpu.config.width = w;
                        gpu.config.height = h;
                        gpu.surface.configure(gpu.renderer.device(), &gpu.config);
                    }
                }
                // winit's Wayland backend only dispatches `RedrawRequested` once
                // the compositor has sent a frame callback, so the surface is
                // ready here (no `Fifo` stall). `render` still recovers if the
                // surface went stale while we were hidden.
                self.render();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        self.clear_expired_status();
        self.pump_frames();
        // Poll the capture channel at ~250 Hz rather than spinning the event
        // loop continuously (a frame at 30 fps arrives every ~33 ms; 4 ms keeps
        // latency low without burning a core under ControlFlow::Poll).
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + std::time::Duration::from_millis(4),
        ));
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        let _ = self.abort_paste("Paste warning: aborted");
        self.teardown_clipboard();
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

fn estimated_paste_duration(outcome: PasteOutcome) -> Duration {
    let millis = (DEFAULT_COMMAND_GAP.as_millis() as u64).saturating_mul(outcome.reports as u64);
    Duration::from_millis(millis)
}

/// How to react to a `Surface::get_current_texture` failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceRecovery {
    /// The surface is outdated; reconfigure and try again.
    Reconfigure,
    /// The surface is lost; recreate it from the window before trying again.
    Recreate,
    /// Transient or fatal-but-rare statuses; skip this frame.
    Skip,
}

/// Maps a surface acquisition status to a recovery action. `Outdated` can reuse
/// the surface after reconfiguration; `Lost` requires recreating the surface.
fn surface_recovery(status: &wgpu::CurrentSurfaceTexture) -> SurfaceRecovery {
    match status {
        wgpu::CurrentSurfaceTexture::Outdated => SurfaceRecovery::Reconfigure,
        wgpu::CurrentSurfaceTexture::Lost => SurfaceRecovery::Recreate,
        _ => SurfaceRecovery::Skip,
    }
}

/// Whether to request client-side window decorations, from
/// `OPENTERFACE_USE_LIBDECOR` (legacy name). Defaults to **off** (undecorated
/// xdg-shell): winit's client-side decorations (CSD) race wgpu's surface
/// presentation on focus/visibility changes and can make the window disappear
/// on CSD-only compositors (niri). The value is matched case-insensitively;
/// unset, empty, or `0`/`false`/`no`/`off` stay off, anything else opts in.
fn decorations_enabled(var: Option<String>) -> bool {
    match var {
        Some(v) => {
            let v = v.trim().to_ascii_lowercase();
            !(v.is_empty() || matches!(v.as_str(), "0" | "false" | "no" | "off"))
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{surface_recovery, SurfaceRecovery};

    #[test]
    fn outdated_surface_reconfigures() {
        assert_eq!(
            surface_recovery(&wgpu::CurrentSurfaceTexture::Outdated),
            SurfaceRecovery::Reconfigure
        );
    }

    #[test]
    fn lost_surface_recreates() {
        assert_eq!(
            surface_recovery(&wgpu::CurrentSurfaceTexture::Lost),
            SurfaceRecovery::Recreate
        );
    }

    #[test]
    fn transient_or_fatal_surface_errors_skip() {
        assert_eq!(
            surface_recovery(&wgpu::CurrentSurfaceTexture::Timeout),
            SurfaceRecovery::Skip
        );
        assert_eq!(
            surface_recovery(&wgpu::CurrentSurfaceTexture::Validation),
            SurfaceRecovery::Skip
        );
    }

    #[test]
    fn decorations_default_off_and_opt_in() {
        use super::decorations_enabled;
        // Default (unset), empty, and whitespace: undecorated, to avoid the
        // CSD↔wgpu unmap race.
        assert!(!decorations_enabled(None));
        assert!(!decorations_enabled(Some(String::new())));
        assert!(!decorations_enabled(Some("   ".to_string())));
        // Explicit "off" spellings stay off (case-insensitive).
        for v in ["0", "false", "FALSE", "No", "off", " Off "] {
            assert!(!decorations_enabled(Some(v.to_string())), "{v:?}");
        }
        // Anything else opts into decorations.
        for v in ["1", "true", "YES", "on"] {
            assert!(decorations_enabled(Some(v.to_string())), "{v:?}");
        }
    }
}
