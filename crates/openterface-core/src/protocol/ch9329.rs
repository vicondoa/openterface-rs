//! CH9329 command framing and command builders.
//!
//! Every command is `57 AB 00 <CMD> <LEN> <DATA..> <SUM>`, where `SUM` is the
//! low byte of the additive sum of all preceding bytes. The builders here are
//! **pure** (no I/O); the pacing scheduler ([`crate::pacing`]) decides *when* to
//! write them and the serial transport ([`crate::serial`]) writes the bytes.
//!
//! Layouts (see `docs/reference/cpp-cli-behavior.md`):
//! - **Absolute mouse** — `CMD=0x04`, data `02 <buttons> <xLo> <xHi> <yLo>
//!   <yHi> <wheel>`, X/Y in `0..=4095` little-endian.
//! - **Relative mouse** — `CMD=0x05`, data `01 <buttons> <dx> <dy> <wheel>`,
//!   `dx`/`dy` signed 8-bit.
//! - **Keyboard** — `CMD=0x02`, data `<mod> 00 <k1>..<k6>` (USB HID usages);
//!   an all-zero report is a release.

use crate::event::{AbsPosition, ButtonMask, HidUsage, Modifiers, ABS_MAX};

/// The 3-byte prefix that begins every CH9329 command frame.
pub const FRAME_PREFIX: [u8; 3] = [0x57, 0xAB, 0x00];

/// Maximum number of simultaneous non-modifier keys in one HID report.
pub const MAX_KEYS: usize = 6;

/// CH9329 command opcodes (subset).
pub mod cmd {
    /// Query chip info (`GET_INFO`). Some firmware does not answer this; a
    /// missing response is harmless and must not be treated as an error.
    pub const GET_INFO: u8 = 0x01;
    /// Standard keyboard report.
    pub const KEYBOARD: u8 = 0x02;
    /// Absolute mouse move/buttons.
    pub const MOUSE_ABS: u8 = 0x04;
    /// Relative mouse move/buttons.
    pub const MOUSE_REL: u8 = 0x05;
    /// Query parameter config (`GET_PARA_CFG`).
    pub const GET_PARA_CFG: u8 = 0x08;
    /// Set parameter config (`SET_PARA_CFG`).
    pub const SET_PARA_CFG: u8 = 0x09;
    /// Software reset.
    pub const RESET: u8 = 0x0F;
}

/// The 50-byte `SET_PARA_CFG` payload that restores the CH9329 to **mode 0x82**
/// (protocol-transfer USB keyboard+mouse) at **115200 baud** — copied verbatim
/// from the C++ `resetChip` reconfiguration so the bytes are known-good for this
/// hardware.
const PARA_CFG_MODE_82: [u8; 50] = [
    0x82, 0x80, 0x00, 0x00, 0x00, 0x01, 0xC2, 0x00, 0x08, 0x00, 0x00, 0x03, 0x86, 0x1A, 0x29, 0xE1,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x0D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00,
];

/// Builds the `SET_PARA_CFG` command (`CMD 0x09`) that restores the CH9329 to
/// mode 0x82 (USB keyboard+mouse) at 115200 baud, matching the C++ reset
/// reconfiguration.
#[must_use]
pub fn set_para_cfg() -> Vec<u8> {
    frame(cmd::SET_PARA_CFG, &PARA_CFG_MODE_82)
}

/// Computes the CH9329 checksum: the low byte of the additive sum of `bytes`.
///
/// # Examples
///
/// ```
/// use openterface_core::protocol::ch9329::checksum;
/// // The 3-byte frame prefix sums to 0x57 + 0xAB + 0x00 = 0x102 -> low byte 0x02.
/// assert_eq!(checksum(&[0x57, 0xAB, 0x00]), 0x02);
/// ```
#[must_use]
pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

/// Frames a command: prefix + opcode + length + data + checksum.
///
/// `data.len()` must fit in a single byte (CH9329 length field); the builders
/// in this module always satisfy that.
#[must_use]
pub fn frame(cmd: u8, data: &[u8]) -> Vec<u8> {
    debug_assert!(data.len() <= u8::MAX as usize);
    let mut out = Vec::with_capacity(FRAME_PREFIX.len() + 2 + data.len() + 1);
    out.extend_from_slice(&FRAME_PREFIX);
    out.push(cmd);
    out.push(data.len() as u8);
    out.extend_from_slice(data);
    let sum = checksum(&out);
    out.push(sum);
    out
}

/// Builds an **absolute** mouse command.
///
/// `pos` is clamped to `0..=4095` per axis; `wheel` is a signed tick
/// (0 for none).
///
/// # Examples
///
/// ```
/// use openterface_core::event::{AbsPosition, ButtonMask};
/// use openterface_core::protocol::ch9329::mouse_absolute;
///
/// let frame = mouse_absolute(AbsPosition { x: 100, y: 200 }, ButtonMask::LEFT, 0);
/// // 57 AB 00 | CMD 04 | LEN 07 | 02 buttons xLo xHi yLo yHi wheel | SUM
/// assert_eq!(
///     frame,
///     vec![0x57, 0xAB, 0x00, 0x04, 0x07, 0x02, 0x01, 0x64, 0x00, 0xC8, 0x00, 0x00, 0x3C],
/// );
/// ```
#[must_use]
pub fn mouse_absolute(pos: AbsPosition, buttons: ButtonMask, wheel: i8) -> Vec<u8> {
    let x = pos.x.min(ABS_MAX);
    let y = pos.y.min(ABS_MAX);
    let data = [
        0x02,
        buttons.bits(),
        (x & 0xFF) as u8,
        (x >> 8) as u8,
        (y & 0xFF) as u8,
        (y >> 8) as u8,
        wheel as u8,
    ];
    frame(cmd::MOUSE_ABS, &data)
}

/// Builds a **relative** mouse command. `dx`/`dy` are signed 8-bit deltas;
/// `wheel` is a signed tick (`0x01` up / `0xFF` down for scroll).
#[must_use]
pub fn mouse_relative(dx: i8, dy: i8, buttons: ButtonMask, wheel: i8) -> Vec<u8> {
    let data = [0x01, buttons.bits(), dx as u8, dy as u8, wheel as u8];
    frame(cmd::MOUSE_REL, &data)
}

/// Builds a keyboard report with the given modifiers and up to [`MAX_KEYS`]
/// held keys (extras beyond six are ignored — N-key rollover is not modeled).
#[must_use]
pub fn keyboard(modifiers: Modifiers, keys: &[HidUsage]) -> Vec<u8> {
    let mut data = [0u8; 2 + MAX_KEYS];
    data[0] = modifiers.0;
    // data[1] is reserved (always 0x00).
    for (slot, key) in data[2..].iter_mut().zip(keys.iter()) {
        *slot = key.0;
    }
    frame(cmd::KEYBOARD, &data)
}

/// Builds an all-zero keyboard report (release all keys and modifiers).
#[must_use]
pub fn keyboard_release() -> Vec<u8> {
    keyboard(Modifiers::NONE, &[])
}

/// Builds the sequence of CH9329 keyboard frames that "types" `text`: for each
/// mappable character, a key-press report (with any needed modifier) followed
/// by an all-keys-released report. Unmappable characters are skipped. This is
/// the C++ `Serial::sendText` behavior.
#[must_use]
pub fn text_to_reports(text: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for ch in text.chars() {
        if let Some((mods, usage)) = crate::protocol::hid::ascii_to_hid(ch) {
            out.push(keyboard(mods, &[usage]));
            out.push(keyboard_release());
        }
    }
    out
}

/// Builds the `GET_INFO` query.
#[must_use]
pub fn get_info() -> Vec<u8> {
    frame(cmd::GET_INFO, &[])
}

/// Builds the software reset command.
#[must_use]
pub fn software_reset() -> Vec<u8> {
    frame(cmd::RESET, &[])
}

/// A parsed CH9329 response frame.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Response {
    /// The response command byte (typically the request cmd with bit 7 set).
    pub cmd: u8,
    /// The response payload (between length and checksum).
    pub data: Vec<u8>,
}

/// Parses and validates a single CH9329 response frame from the start of
/// `bytes`. Returns the response and the number of bytes consumed, or `None`
/// if `bytes` does not begin with a complete, checksum-valid frame.
#[must_use]
pub fn parse_response(bytes: &[u8]) -> Option<(Response, usize)> {
    if bytes.len() < 6 || bytes[0..3] != FRAME_PREFIX {
        return None;
    }
    let cmd = bytes[3];
    let len = bytes[4] as usize;
    let total = 5 + len + 1;
    if bytes.len() < total {
        return None;
    }
    let sum = checksum(&bytes[..total - 1]);
    if sum != bytes[total - 1] {
        return None;
    }
    Some((
        Response {
            cmd,
            data: bytes[5..5 + len].to_vec(),
        },
        total,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_wraps() {
        assert_eq!(checksum(&[0x57, 0xAB, 0x00]), 0x02);
        assert_eq!(checksum(&[0xFF, 0x01]), 0x00);
        assert_eq!(checksum(&[]), 0x00);
    }

    #[test]
    fn frame_has_prefix_len_and_checksum() {
        let f = frame(cmd::GET_INFO, &[]);
        assert_eq!(&f[..5], &[0x57, 0xAB, 0x00, 0x01, 0x00]);
        assert_eq!(*f.last().unwrap(), checksum(&f[..f.len() - 1]));
        assert_eq!(*f.last().unwrap(), 0x03);
    }

    // Golden vector from PROGRESS.md: absolute mouse at (100, 200), left button.
    #[test]
    fn golden_absolute_mouse() {
        let f = mouse_absolute(AbsPosition { x: 100, y: 200 }, ButtonMask::LEFT, 0);
        assert_eq!(
            f,
            vec![0x57, 0xAB, 0x00, 0x04, 0x07, 0x02, 0x01, 0x64, 0x00, 0xC8, 0x00, 0x00, 0x3C]
        );
    }

    #[test]
    fn absolute_mouse_clamps_to_4095() {
        let f = mouse_absolute(AbsPosition { x: 9000, y: 4095 }, ButtonMask::NONE, 0);
        // x clamped to 4095 = 0x0FFF -> lo 0xFF hi 0x0F.
        assert_eq!(&f[6..12], &[0x00, 0xFF, 0x0F, 0xFF, 0x0F, 0x00]);
    }

    // dx=5, dy=-3, left button, no wheel.
    #[test]
    fn golden_relative_mouse() {
        let f = mouse_relative(5, -3, ButtonMask::LEFT, 0);
        assert_eq!(
            f,
            vec![0x57, 0xAB, 0x00, 0x05, 0x05, 0x01, 0x01, 0x05, 0xFD, 0x00, 0x10]
        );
    }

    #[test]
    fn relative_scroll_wheel_byte() {
        let up = mouse_relative(0, 0, ButtonMask::NONE, 1);
        let down = mouse_relative(0, 0, ButtonMask::NONE, -1);
        assert_eq!(up[9], 0x01);
        assert_eq!(down[9], 0xFF);
    }

    // Golden vector: key 'a' (HID 0x04), no modifiers.
    #[test]
    fn golden_keyboard_single_key() {
        let f = keyboard(Modifiers::NONE, &[HidUsage(0x04)]);
        assert_eq!(
            f,
            vec![
                0x57, 0xAB, 0x00, 0x02, 0x08, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10
            ]
        );
    }

    // Ctrl+Alt+Del: modifiers 0x05 (Ctrl+Alt), key 0x4C.
    #[test]
    fn golden_ctrl_alt_del() {
        let f = keyboard(
            Modifiers::LEFT_CTRL.union(Modifiers::LEFT_ALT),
            &[HidUsage(0x4C)],
        );
        assert_eq!(f[5], 0x05);
        assert_eq!(f[7], 0x4C);
        assert_eq!(*f.last().unwrap(), 0x5D);
    }

    #[test]
    fn keyboard_release_is_all_zero_report() {
        let f = keyboard_release();
        assert_eq!(
            f,
            vec![
                0x57, 0xAB, 0x00, 0x02, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C
            ]
        );
    }

    #[test]
    fn text_to_reports_types_press_release_per_char() {
        // "Ab1" -> shift+a, release, b, release, 1, release.
        let r = text_to_reports("Ab1");
        assert_eq!(r.len(), 6);
        assert_eq!(r[0], keyboard(Modifiers::LEFT_SHIFT, &[HidUsage(0x04)])); // 'A'
        assert_eq!(r[1], keyboard_release());
        assert_eq!(r[2], keyboard(Modifiers::NONE, &[HidUsage(0x05)])); // 'b'
        assert_eq!(r[3], keyboard_release());
        assert_eq!(r[4], keyboard(Modifiers::NONE, &[HidUsage(0x1E)])); // '1'
        assert_eq!(r[5], keyboard_release());
    }

    #[test]
    fn text_to_reports_skips_unmappable() {
        // A control char with no HID mapping is skipped.
        assert!(text_to_reports("\u{0}").is_empty());
    }

    #[test]
    fn set_para_cfg_is_mode_82_115200() {
        let f = set_para_cfg();
        assert_eq!(f.len(), 5 + 50 + 1);
        assert_eq!(&f[0..5], &[0x57, 0xAB, 0x00, 0x09, 0x32]);
        assert_eq!(f[5], 0x82); // working mode byte
        assert_eq!(&f[9..13], &[0x00, 0x01, 0xC2, 0x00]); // 115200 baud region
        let sum = checksum(&f[..f.len() - 1]);
        assert_eq!(*f.last().unwrap(), sum);
    }

    #[test]
    fn keyboard_caps_at_six_keys() {
        let keys: Vec<HidUsage> = (0..10).map(HidUsage).collect();
        let f = keyboard(Modifiers::NONE, &keys);
        // LEN must remain 8 (mod + reserved + 6 keys), only first 6 keys used.
        assert_eq!(f[4], 0x08);
        assert_eq!(&f[7..13], &[0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn software_reset_frame() {
        let f = software_reset();
        assert_eq!(&f[..5], &[0x57, 0xAB, 0x00, 0x0F, 0x00]);
    }

    #[test]
    fn parse_valid_response() {
        // GET_INFO response: cmd 0x81, 1 data byte.
        let f = frame(0x81, &[0x42]);
        let (resp, consumed) = parse_response(&f).unwrap();
        assert_eq!(resp.cmd, 0x81);
        assert_eq!(resp.data, vec![0x42]);
        assert_eq!(consumed, f.len());
    }

    #[test]
    fn parse_rejects_bad_checksum_and_partial() {
        let mut f = frame(0x81, &[0x42]);
        let n = f.len();
        f[n - 1] ^= 0xFF; // corrupt checksum
        assert!(parse_response(&f).is_none());
        // Truncated frame.
        let g = frame(0x81, &[0x42]);
        assert!(parse_response(&g[..g.len() - 1]).is_none());
        // Wrong prefix.
        assert!(parse_response(&[0x00, 0x00, 0x00, 0x81, 0x00, 0x81]).is_none());
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn frame_is_always_well_formed(cmd in any::<u8>(), data in prop::collection::vec(any::<u8>(), 0..=250)) {
            let f = frame(cmd, &data);
            prop_assert_eq!(f[0], 0x57);
            prop_assert_eq!(f[1], 0xAB);
            prop_assert_eq!(f[2], 0x00);
            prop_assert_eq!(f[3], cmd);
            prop_assert_eq!(f[4] as usize, data.len());
            prop_assert_eq!(f.len(), 6 + data.len());
            prop_assert_eq!(*f.last().unwrap(), checksum(&f[..f.len() - 1]));
        }

        #[test]
        fn absolute_coords_are_always_clamped(x in any::<u16>(), y in any::<u16>()) {
            let f = mouse_absolute(AbsPosition { x, y }, ButtonMask::NONE, 0);
            let xx = u16::from(f[7]) | (u16::from(f[8]) << 8);
            let yy = u16::from(f[9]) | (u16::from(f[10]) << 8);
            prop_assert!(xx <= ABS_MAX);
            prop_assert!(yy <= ABS_MAX);
        }
    }
}
