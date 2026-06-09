//! The device-agnostic input-event model.
//!
//! The GUI produces these events from window input; the [`crate::input`] and
//! [`crate::pacing`] layers translate and pace them into CH9329 commands. Types
//! here are intentionally free of any windowing or protocol detail.

/// A USB HID keyboard usage id (Usage Page 0x07), e.g. `0x28` = Enter.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct HidUsage(pub u8);

/// CH9329 keyboard modifier bitmask.
///
/// Bit layout matches the USB HID boot-keyboard modifier byte.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Modifiers(pub u8);

impl Modifiers {
    /// No modifiers held.
    pub const NONE: Modifiers = Modifiers(0x00);
    /// Left Control.
    pub const LEFT_CTRL: Modifiers = Modifiers(0x01);
    /// Left Shift.
    pub const LEFT_SHIFT: Modifiers = Modifiers(0x02);
    /// Left Alt.
    pub const LEFT_ALT: Modifiers = Modifiers(0x04);
    /// Left GUI (Super/Meta).
    pub const LEFT_GUI: Modifiers = Modifiers(0x08);
    /// Right Control.
    pub const RIGHT_CTRL: Modifiers = Modifiers(0x10);
    /// Right Shift.
    pub const RIGHT_SHIFT: Modifiers = Modifiers(0x20);
    /// Right Alt (AltGr).
    pub const RIGHT_ALT: Modifiers = Modifiers(0x40);
    /// Right GUI.
    pub const RIGHT_GUI: Modifiers = Modifiers(0x80);

    /// Returns `true` if every bit in `other` is set in `self`.
    #[must_use]
    pub fn contains(self, other: Modifiers) -> bool {
        self.0 & other.0 == other.0
    }

    /// Returns the union of two modifier sets.
    #[must_use]
    pub fn union(self, other: Modifiers) -> Modifiers {
        Modifiers(self.0 | other.0)
    }
}

/// A pointer button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseButton {
    /// Left button (CH9329 bit 0).
    Left,
    /// Right button (CH9329 bit 1).
    Right,
    /// Middle button (CH9329 bit 2).
    Middle,
}

/// CH9329 mouse button bitmask: bit0 left, bit1 right, bit2 middle.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct ButtonMask(pub u8);

impl ButtonMask {
    /// No buttons held.
    pub const NONE: ButtonMask = ButtonMask(0x00);
    /// Left button bit.
    pub const LEFT: ButtonMask = ButtonMask(0x01);
    /// Right button bit.
    pub const RIGHT: ButtonMask = ButtonMask(0x02);
    /// Middle button bit.
    pub const MIDDLE: ButtonMask = ButtonMask(0x04);

    /// The mask bit for a single button.
    #[must_use]
    pub fn from_button(button: MouseButton) -> ButtonMask {
        match button {
            MouseButton::Left => ButtonMask::LEFT,
            MouseButton::Right => ButtonMask::RIGHT,
            MouseButton::Middle => ButtonMask::MIDDLE,
        }
    }

    /// Returns a copy with `button` set or cleared.
    #[must_use]
    pub fn with(self, button: MouseButton, pressed: bool) -> ButtonMask {
        let bit = ButtonMask::from_button(button).0;
        if pressed {
            ButtonMask(self.0 | bit)
        } else {
            ButtonMask(self.0 & !bit)
        }
    }

    /// The raw bitmask byte.
    #[must_use]
    pub fn bits(self) -> u8 {
        self.0
    }
}

/// An absolute pointer position in the CH9329 `0..=4095` coordinate space.
///
/// The GUI maps window-relative coordinates into this space before forwarding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AbsPosition {
    /// X in `0..=4095`.
    pub x: u16,
    /// Y in `0..=4095`.
    pub y: u16,
}

/// The maximum value of an absolute coordinate axis (12-bit).
pub const ABS_MAX: u16 = 4095;

/// A single device-agnostic input event to forward to the target.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputEvent {
    /// A key press or release with the modifier set held at the time.
    Key {
        /// The HID usage of the key.
        usage: HidUsage,
        /// Modifiers held with this key.
        modifiers: Modifiers,
        /// `true` for press, `false` for release.
        pressed: bool,
    },
    /// Absolute pointer move.
    MouseMoveAbsolute {
        /// Target position.
        pos: AbsPosition,
    },
    /// Relative pointer move (delta).
    MouseMoveRelative {
        /// Delta X.
        dx: i16,
        /// Delta Y.
        dy: i16,
    },
    /// A pointer button press or release.
    MouseButton {
        /// Which button.
        button: MouseButton,
        /// `true` for press, `false` for release.
        pressed: bool,
    },
    /// A vertical scroll tick (positive = up).
    Scroll {
        /// Wheel delta.
        delta: i8,
    },
    /// Release **all** held keys, modifiers, and mouse buttons.
    ///
    /// Sent when the window loses focus or the pointer leaves, so the target
    /// never sees a key/button stuck down.
    ReleaseAll,
}

impl InputEvent {
    /// Returns `true` if this event is a key/button **release**.
    ///
    /// Releases are latency-critical: the pacing scheduler must never let them
    /// queue behind a backlog of mouse-move commands, or the target sees a key
    /// held too long (autorepeat) or a click without a clean up-edge.
    #[must_use]
    pub fn is_release(self) -> bool {
        matches!(
            self,
            InputEvent::Key { pressed: false, .. }
                | InputEvent::MouseButton { pressed: false, .. }
                | InputEvent::ReleaseAll
        )
    }

    /// Returns `true` if this event is a mouse move (absolute or relative).
    ///
    /// Move events are coalescible: only the latest position matters.
    #[must_use]
    pub fn is_mouse_move(self) -> bool {
        matches!(
            self,
            InputEvent::MouseMoveAbsolute { .. } | InputEvent::MouseMoveRelative { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_detection() {
        let press = InputEvent::Key {
            usage: HidUsage(0x04),
            modifiers: Modifiers::NONE,
            pressed: true,
        };
        let release = InputEvent::Key {
            usage: HidUsage(0x04),
            modifiers: Modifiers::NONE,
            pressed: false,
        };
        assert!(!press.is_release());
        assert!(release.is_release());
    }

    #[test]
    fn modifier_set_ops() {
        let cs = Modifiers::LEFT_CTRL.union(Modifiers::LEFT_SHIFT);
        assert!(cs.contains(Modifiers::LEFT_CTRL));
        assert!(cs.contains(Modifiers::LEFT_SHIFT));
        assert!(!cs.contains(Modifiers::LEFT_ALT));
    }
}
