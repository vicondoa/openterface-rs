//! CH9329 command framing.
//!
//! Every command is `57 AB 00 <CMD> <LEN> <DATA..> <SUM>`, where `SUM` is the
//! low byte of the additive sum of all preceding bytes. Full command builders
//! (absolute/relative mouse, keyboard reports, GET_INFO, factory reset) and the
//! response decoder land in **W2.1**; W0 provides the frame constants and the
//! checksum primitive (with tests) so the skeleton ships a green test.

/// The 3-byte prefix that begins every CH9329 command frame.
pub const FRAME_PREFIX: [u8; 3] = [0x57, 0xAB, 0x00];

/// CH9329 command opcodes (subset; extended in W2.1).
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
}

/// Computes the CH9329 checksum: the low byte of the additive sum of `bytes`.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

/// Frames a command: prefix + opcode + length + data + checksum.
///
/// Returns the complete on-wire byte sequence. `data.len()` must fit in a
/// single byte (CH9329 length field); callers in W2.1 enforce this.
#[must_use]
pub fn frame(cmd: u8, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_PREFIX.len() + 2 + data.len() + 1);
    out.extend_from_slice(&FRAME_PREFIX);
    out.push(cmd);
    out.push(data.len() as u8);
    out.extend_from_slice(data);
    let sum = checksum(&out);
    out.push(sum);
    out
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
        // GET_INFO with empty payload: 57 AB 00 01 00 <sum>.
        let f = frame(cmd::GET_INFO, &[]);
        assert_eq!(&f[..5], &[0x57, 0xAB, 0x00, 0x01, 0x00]);
        assert_eq!(*f.last().unwrap(), checksum(&f[..f.len() - 1]));
        // 57+AB+00+01+00 = 0x103 -> low byte 0x03.
        assert_eq!(*f.last().unwrap(), 0x03);
    }

    #[test]
    fn frame_length_field_matches_payload() {
        let f = frame(
            cmd::KEYBOARD,
            &[0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00],
        );
        assert_eq!(f[3], cmd::KEYBOARD);
        assert_eq!(f[4], 8);
    }
}
