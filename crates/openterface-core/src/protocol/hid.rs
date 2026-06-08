//! USB HID usage tables and key mapping.
//!
//! Two paths, matching the C++ implementation (see
//! `docs/reference/cpp-cli-behavior.md` -> keyboard translation):
//!
//! - **physical-key forwarding** - a Linux evdev scancode -> USB HID usage table
//!   ([`evdev_to_hid`]), so the target sees the same *physical* key regardless
//!   of host layout. Modifier keys are reported via the CH9329 modifier byte
//!   ([`modifier_bit`]), not in the 6-key array ([`is_modifier`]).
//! - **text injection** (`sendText`) - an ASCII -> (modifiers, usage) map
//!   ([`ascii_to_hid`]) for typing a string on a US layout.
//!
//! Usage values are USB HID Usage Page 0x07 (Keyboard/Keypad).

use crate::event::{HidUsage, Modifiers};

/// Returns `true` if `usage` is a modifier key (LeftCtrl..RightGui,
/// `0xE0..=0xE7`). Modifier keys are tracked via the modifier byte and are not
/// placed in the 6-key report array.
#[must_use]
pub fn is_modifier(usage: HidUsage) -> bool {
    (0xE0..=0xE7).contains(&usage.0)
}

/// Maps a modifier HID usage (`0xE0..=0xE7`) to its CH9329 modifier bit.
///
/// The CH9329 modifier byte uses the same bit layout as the USB HID boot
/// keyboard, so this is a direct positional mapping (LeftCtrl -> `0x01`, ...,
/// RightGui -> `0x80`).
#[must_use]
pub fn modifier_bit(usage: HidUsage) -> Option<Modifiers> {
    if is_modifier(usage) {
        Some(Modifiers(1 << (usage.0 - 0xE0)))
    } else {
        None
    }
}

/// Maps a Linux evdev scancode (`linux/input-event-codes.h` `KEY_*`) to a USB
/// HID usage. Returns `None` for keys without a standard HID usage.
///
/// The GUI obtains the evdev code from the Wayland/`winit` key event (a Wayland
/// keycode is `evdev + 8`).
#[must_use]
pub fn evdev_to_hid(code: u16) -> Option<HidUsage> {
    let usage: u8 = match code {
        1 => 0x29,  // Esc
        2 => 0x1E,  // 1
        3 => 0x1F,  // 2
        4 => 0x20,  // 3
        5 => 0x21,  // 4
        6 => 0x22,  // 5
        7 => 0x23,  // 6
        8 => 0x24,  // 7
        9 => 0x25,  // 8
        10 => 0x26, // 9
        11 => 0x27, // 0
        12 => 0x2D, // -
        13 => 0x2E, // =
        14 => 0x2A, // Backspace
        15 => 0x2B, // Tab
        16 => 0x14, // q
        17 => 0x1A, // w
        18 => 0x08, // e
        19 => 0x15, // r
        20 => 0x17, // t
        21 => 0x1C, // y
        22 => 0x18, // u
        23 => 0x0C, // i
        24 => 0x12, // o
        25 => 0x13, // p
        26 => 0x2F, // [
        27 => 0x30, // ]
        28 => 0x28, // Enter
        29 => 0xE0, // LeftCtrl
        30 => 0x04, // a
        31 => 0x16, // s
        32 => 0x07, // d
        33 => 0x09, // f
        34 => 0x0A, // g
        35 => 0x0B, // h
        36 => 0x0D, // j
        37 => 0x0E, // k
        38 => 0x0F, // l
        39 => 0x33, // ;
        40 => 0x34, // '
        41 => 0x35, // `
        42 => 0xE1, // LeftShift
        43 => 0x32, // backslash (C++ table maps KEY_BACKSLASH to Non-US 0x32)
        44 => 0x1D, // z
        45 => 0x1B, // x
        46 => 0x06, // c
        47 => 0x19, // v
        48 => 0x05, // b
        49 => 0x11, // n
        50 => 0x10, // m
        51 => 0x36, // ,
        52 => 0x37, // .
        53 => 0x38, // /
        54 => 0xE5, // RightShift
        55 => 0x55, // KP *
        56 => 0xE2, // LeftAlt
        57 => 0x2C, // Space
        58 => 0x39, // CapsLock
        59 => 0x3A, // F1
        60 => 0x3B, // F2
        61 => 0x3C, // F3
        62 => 0x3D, // F4
        63 => 0x3E, // F5
        64 => 0x3F, // F6
        65 => 0x40, // F7
        66 => 0x41, // F8
        67 => 0x42, // F9
        68 => 0x43, // F10
        69 => 0x53, // NumLock
        70 => 0x47, // ScrollLock
        71 => 0x5F, // KP 7
        72 => 0x60, // KP 8
        73 => 0x61, // KP 9
        74 => 0x56, // KP -
        75 => 0x5C, // KP 4
        76 => 0x5D, // KP 5
        77 => 0x5E, // KP 6
        78 => 0x57, // KP +
        79 => 0x59, // KP 1
        80 => 0x5A, // KP 2
        81 => 0x5B, // KP 3
        82 => 0x62, // KP 0
        83 => 0x63, // KP .
        86 => 0x64, // KEY_102ND: ISO extra key — intentional additive divergence
        // from the C++ table (which omits it); see cpp-cli-behavior.md.
        87 => 0x44,  // F11
        88 => 0x45,  // F12
        96 => 0x58,  // KP Enter
        97 => 0xE4,  // RightCtrl
        98 => 0x54,  // KP /
        99 => 0x46,  // PrintScreen (SysRq)
        100 => 0xE6, // RightAlt
        102 => 0x4A, // Home
        103 => 0x52, // Up
        104 => 0x4B, // PageUp
        105 => 0x50, // Left
        106 => 0x4F, // Right
        107 => 0x4D, // End
        108 => 0x51, // Down
        109 => 0x4E, // PageDown
        110 => 0x49, // Insert
        111 => 0x4C, // Delete
        119 => 0x48, // Pause
        125 => 0xE3, // LeftMeta (Super)
        126 => 0xE7, // RightMeta
        127 => 0x65, // Menu (Application)
        _ => return None,
    };
    Some(HidUsage(usage))
}

/// Maps a printable ASCII character to the `(modifiers, usage)` needed to type
/// it on a **US layout** (for the `sendText` path). Returns `None` for
/// characters without a single-keystroke US mapping.
#[must_use]
pub fn ascii_to_hid(c: char) -> Option<(Modifiers, HidUsage)> {
    let none = Modifiers::NONE;
    let shift = Modifiers::LEFT_SHIFT;
    let (mods, usage): (Modifiers, u8) = match c {
        'a'..='z' => (none, 0x04 + (c as u8 - b'a')),
        'A'..='Z' => (shift, 0x04 + (c as u8 - b'A')),
        '1'..='9' => (none, 0x1E + (c as u8 - b'1')),
        '0' => (none, 0x27),
        ' ' => (none, 0x2C),
        '\n' => (none, 0x28), // Enter
        '\t' => (none, 0x2B), // Tab
        '-' => (none, 0x2D),
        '=' => (none, 0x2E),
        '[' => (none, 0x2F),
        ']' => (none, 0x30),
        '\\' => (none, 0x31),
        ';' => (none, 0x33),
        '\'' => (none, 0x34),
        '`' => (none, 0x35),
        ',' => (none, 0x36),
        '.' => (none, 0x37),
        '/' => (none, 0x38),
        '!' => (shift, 0x1E),
        '@' => (shift, 0x1F),
        '#' => (shift, 0x20),
        '$' => (shift, 0x21),
        '%' => (shift, 0x22),
        '^' => (shift, 0x23),
        '&' => (shift, 0x24),
        '*' => (shift, 0x25),
        '(' => (shift, 0x26),
        ')' => (shift, 0x27),
        '_' => (shift, 0x2D),
        '+' => (shift, 0x2E),
        '{' => (shift, 0x2F),
        '}' => (shift, 0x30),
        '|' => (shift, 0x31),
        ':' => (shift, 0x33),
        '"' => (shift, 0x34),
        '~' => (shift, 0x35),
        '<' => (shift, 0x36),
        '>' => (shift, 0x37),
        '?' => (shift, 0x38),
        _ => return None,
    };
    Some((mods, HidUsage(usage)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_classification_and_bits() {
        assert!(is_modifier(HidUsage(0xE0)));
        assert!(is_modifier(HidUsage(0xE7)));
        assert!(!is_modifier(HidUsage(0x04)));
        assert_eq!(modifier_bit(HidUsage(0xE0)), Some(Modifiers::LEFT_CTRL));
        assert_eq!(modifier_bit(HidUsage(0xE1)), Some(Modifiers::LEFT_SHIFT));
        assert_eq!(modifier_bit(HidUsage(0xE2)), Some(Modifiers::LEFT_ALT));
        assert_eq!(modifier_bit(HidUsage(0xE3)), Some(Modifiers::LEFT_GUI));
        assert_eq!(modifier_bit(HidUsage(0xE7)), Some(Modifiers::RIGHT_GUI));
        assert_eq!(modifier_bit(HidUsage(0x04)), None);
    }

    #[test]
    fn evdev_table_spot_checks() {
        assert_eq!(evdev_to_hid(30), Some(HidUsage(0x04))); // a
        assert_eq!(evdev_to_hid(44), Some(HidUsage(0x1D))); // z
        assert_eq!(evdev_to_hid(2), Some(HidUsage(0x1E))); // 1
        assert_eq!(evdev_to_hid(11), Some(HidUsage(0x27))); // 0
        assert_eq!(evdev_to_hid(28), Some(HidUsage(0x28))); // Enter
        assert_eq!(evdev_to_hid(1), Some(HidUsage(0x29))); // Esc
        assert_eq!(evdev_to_hid(57), Some(HidUsage(0x2C))); // Space
        assert_eq!(evdev_to_hid(59), Some(HidUsage(0x3A))); // F1
        assert_eq!(evdev_to_hid(103), Some(HidUsage(0x52))); // Up
        assert_eq!(evdev_to_hid(111), Some(HidUsage(0x4C))); // Delete
        assert_eq!(evdev_to_hid(29), Some(HidUsage(0xE0))); // LeftCtrl
        assert_eq!(evdev_to_hid(43), Some(HidUsage(0x32))); // backslash (C++ parity)
        assert_eq!(evdev_to_hid(86), Some(HidUsage(0x64))); // KEY_102ND (ISO)
        assert_eq!(evdev_to_hid(0xFFFF), None);
    }

    #[test]
    fn evdev_letters_are_modifier_aware() {
        assert!(!is_modifier(evdev_to_hid(30).unwrap()));
        assert!(is_modifier(evdev_to_hid(29).unwrap()));
        assert!(is_modifier(evdev_to_hid(42).unwrap()));
    }

    #[test]
    fn ascii_text_mapping() {
        assert_eq!(ascii_to_hid('a'), Some((Modifiers::NONE, HidUsage(0x04))));
        assert_eq!(ascii_to_hid('z'), Some((Modifiers::NONE, HidUsage(0x1D))));
        assert_eq!(
            ascii_to_hid('A'),
            Some((Modifiers::LEFT_SHIFT, HidUsage(0x04)))
        );
        assert_eq!(ascii_to_hid('0'), Some((Modifiers::NONE, HidUsage(0x27))));
        assert_eq!(
            ascii_to_hid('!'),
            Some((Modifiers::LEFT_SHIFT, HidUsage(0x1E)))
        );
        assert_eq!(ascii_to_hid(' '), Some((Modifiers::NONE, HidUsage(0x2C))));
        assert_eq!(ascii_to_hid('\n'), Some((Modifiers::NONE, HidUsage(0x28))));
        assert_eq!(
            ascii_to_hid('?'),
            Some((Modifiers::LEFT_SHIFT, HidUsage(0x38)))
        );
        assert_eq!(ascii_to_hid('\u{20ac}'), None); // euro sign: no US single-key
    }

    #[test]
    fn basic_sentence_maps_fully() {
        for c in "Hello, World! 123".chars() {
            assert!(ascii_to_hid(c).is_some(), "no mapping for {c:?}");
        }
    }
}
