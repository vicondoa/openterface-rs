//! winit input events → core [`InputEvent`] mapping (behind `display`).
//!
//! Keyboard uses the *physical* key (`winit` `KeyCode`) mapped to a USB-HID
//! usage, so the target sees the same physical key regardless of host layout.
//! Modifiers are tracked and sent in the CH9329 modifier byte.

use openterface_core::event::{HidUsage, Modifiers, MouseButton};
use winit::event::MouseButton as WinitButton;
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

/// Maps a `winit` [`KeyCode`] (W3C `code`) to a USB HID usage (Usage Page 0x07).
#[must_use]
pub(crate) fn keycode_to_hid(code: KeyCode) -> Option<HidUsage> {
    use KeyCode::*;
    let u: u8 = match code {
        KeyA => 0x04,
        KeyB => 0x05,
        KeyC => 0x06,
        KeyD => 0x07,
        KeyE => 0x08,
        KeyF => 0x09,
        KeyG => 0x0A,
        KeyH => 0x0B,
        KeyI => 0x0C,
        KeyJ => 0x0D,
        KeyK => 0x0E,
        KeyL => 0x0F,
        KeyM => 0x10,
        KeyN => 0x11,
        KeyO => 0x12,
        KeyP => 0x13,
        KeyQ => 0x14,
        KeyR => 0x15,
        KeyS => 0x16,
        KeyT => 0x17,
        KeyU => 0x18,
        KeyV => 0x19,
        KeyW => 0x1A,
        KeyX => 0x1B,
        KeyY => 0x1C,
        KeyZ => 0x1D,
        Digit1 => 0x1E,
        Digit2 => 0x1F,
        Digit3 => 0x20,
        Digit4 => 0x21,
        Digit5 => 0x22,
        Digit6 => 0x23,
        Digit7 => 0x24,
        Digit8 => 0x25,
        Digit9 => 0x26,
        Digit0 => 0x27,
        Enter => 0x28,
        Escape => 0x29,
        Backspace => 0x2A,
        Tab => 0x2B,
        Space => 0x2C,
        Minus => 0x2D,
        Equal => 0x2E,
        BracketLeft => 0x2F,
        BracketRight => 0x30,
        Backslash => 0x31,
        Semicolon => 0x33,
        Quote => 0x34,
        Backquote => 0x35,
        Comma => 0x36,
        Period => 0x37,
        Slash => 0x38,
        CapsLock => 0x39,
        F1 => 0x3A,
        F2 => 0x3B,
        F3 => 0x3C,
        F4 => 0x3D,
        F5 => 0x3E,
        F6 => 0x3F,
        F7 => 0x40,
        F8 => 0x41,
        F9 => 0x42,
        F10 => 0x43,
        F11 => 0x44,
        F12 => 0x45,
        PrintScreen => 0x46,
        ScrollLock => 0x47,
        Pause => 0x48,
        Insert => 0x49,
        Home => 0x4A,
        PageUp => 0x4B,
        Delete => 0x4C,
        End => 0x4D,
        PageDown => 0x4E,
        ArrowRight => 0x4F,
        ArrowLeft => 0x50,
        ArrowDown => 0x51,
        ArrowUp => 0x52,
        NumLock => 0x53,
        NumpadDivide => 0x54,
        NumpadMultiply => 0x55,
        NumpadSubtract => 0x56,
        NumpadAdd => 0x57,
        NumpadEnter => 0x58,
        Numpad1 => 0x59,
        Numpad2 => 0x5A,
        Numpad3 => 0x5B,
        Numpad4 => 0x5C,
        Numpad5 => 0x5D,
        Numpad6 => 0x5E,
        Numpad7 => 0x5F,
        Numpad8 => 0x60,
        Numpad9 => 0x61,
        Numpad0 => 0x62,
        NumpadDecimal => 0x63,
        IntlBackslash => 0x64,
        ControlLeft => 0xE0,
        ShiftLeft => 0xE1,
        AltLeft => 0xE2,
        SuperLeft => 0xE3,
        ControlRight => 0xE4,
        ShiftRight => 0xE5,
        AltRight => 0xE6,
        SuperRight => 0xE7,
        _ => return None,
    };
    Some(HidUsage(u))
}

/// Maps a physical key to a HID usage.
#[must_use]
pub(crate) fn physical_to_hid(key: PhysicalKey) -> Option<HidUsage> {
    match key {
        PhysicalKey::Code(code) => keycode_to_hid(code),
        PhysicalKey::Unidentified(_) => None,
    }
}

/// Converts a `winit` [`ModifiersState`] to the CH9329 modifier byte.
#[must_use]
pub(crate) fn modifiers_from_winit(state: ModifiersState) -> Modifiers {
    let mut m = Modifiers::NONE;
    if state.control_key() {
        m = m.union(Modifiers::LEFT_CTRL);
    }
    if state.shift_key() {
        m = m.union(Modifiers::LEFT_SHIFT);
    }
    if state.alt_key() {
        m = m.union(Modifiers::LEFT_ALT);
    }
    if state.super_key() {
        m = m.union(Modifiers::LEFT_GUI);
    }
    m
}

/// Maps a `winit` mouse button to a core [`MouseButton`], if supported.
#[must_use]
pub(crate) fn mouse_button(button: WinitButton) -> Option<MouseButton> {
    match button {
        WinitButton::Left => Some(MouseButton::Left),
        WinitButton::Right => Some(MouseButton::Right),
        WinitButton::Middle => Some(MouseButton::Middle),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_specials_map() {
        assert_eq!(keycode_to_hid(KeyCode::KeyA), Some(HidUsage(0x04)));
        assert_eq!(keycode_to_hid(KeyCode::Enter), Some(HidUsage(0x28)));
        assert_eq!(keycode_to_hid(KeyCode::Escape), Some(HidUsage(0x29)));
        assert_eq!(keycode_to_hid(KeyCode::Delete), Some(HidUsage(0x4C)));
        assert_eq!(keycode_to_hid(KeyCode::ControlLeft), Some(HidUsage(0xE0)));
    }

    #[test]
    fn modifiers_convert() {
        let mut s = ModifiersState::empty();
        s.insert(ModifiersState::CONTROL);
        s.insert(ModifiersState::ALT);
        let m = modifiers_from_winit(s);
        assert!(m.contains(Modifiers::LEFT_CTRL));
        assert!(m.contains(Modifiers::LEFT_ALT));
        assert!(!m.contains(Modifiers::LEFT_SHIFT));
    }

    #[test]
    fn buttons_map() {
        assert_eq!(mouse_button(WinitButton::Left), Some(MouseButton::Left));
        assert_eq!(mouse_button(WinitButton::Back), None);
    }
}
