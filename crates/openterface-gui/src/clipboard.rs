//! Focused Wayland clipboard access for GUI paste.

use std::sync::{Arc, Mutex};

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use winit::window::Window;

/// Focused clipboard read failures. Messages must never include clipboard text.
#[derive(Debug)]
pub(crate) enum ClipboardError {
    UnsupportedDisplay,
    DisplayHandle,
    Empty,
    Load,
    PrimarySelection,
}

impl ClipboardError {
    pub(crate) fn category(&self) -> &'static str {
        match self {
            ClipboardError::UnsupportedDisplay => "unsupported-display",
            ClipboardError::DisplayHandle => "display-handle",
            ClipboardError::Empty => "empty",
            ClipboardError::Load => "load",
            ClipboardError::PrimarySelection => "primary-selection",
        }
    }
}

#[derive(Clone)]
pub(crate) struct ClipboardReader {
    inner: Arc<Mutex<smithay_clipboard::Clipboard>>,
}

impl ClipboardReader {
    pub(crate) fn from_window(window: &Window) -> Result<Self, ClipboardError> {
        let display = window
            .display_handle()
            .map_err(|_| ClipboardError::DisplayHandle)?;
        let RawDisplayHandle::Wayland(handle) = display.as_raw() else {
            return Err(ClipboardError::UnsupportedDisplay);
        };
        // SAFETY: `handle.display` comes from the live winit Wayland window. The
        // reader is stored in `App` and explicitly dropped during app teardown,
        // before the event loop/display is destroyed.
        let inner = unsafe { smithay_clipboard::Clipboard::new(handle.display.as_ptr()) };
        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    pub(crate) fn load_regular(&self) -> Result<String, ClipboardError> {
        let text = self
            .inner
            .lock()
            .map_err(|_| ClipboardError::Load)?
            .load()
            .map_err(|_| ClipboardError::Load)?;
        if text.is_empty() {
            return Err(ClipboardError::Empty);
        }
        Ok(text)
    }

    pub(crate) fn load_primary(&self) -> Result<String, ClipboardError> {
        let text = self
            .inner
            .lock()
            .map_err(|_| ClipboardError::PrimarySelection)?
            .load_primary()
            .map_err(|_| ClipboardError::PrimarySelection)?;
        if text.is_empty() {
            return Err(ClipboardError::Empty);
        }
        Ok(text)
    }
}
