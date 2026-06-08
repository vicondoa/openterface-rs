//! Byte-level serial transport contract for the CH9329 link, plus the
//! connection / baud-fallback state machine.
//!
//! The [`SerialTransport`] contract is deliberately raw (open/read/write/baud).
//! CH9329 *framing* lives in [`crate::protocol::ch9329`] and the *pacing* of
//! writes lives in [`crate::pacing`]; this trait only moves bytes.
//!
//! [`connect_with_fallback`] is the pure negotiation logic: probe at 115200,
//! fall back to 9600, and — because some firmware silently ignores `GET_INFO` —
//! treat a non-responsive chip at the primary rate as a successful (if
//! unverified) connection rather than an error.

use std::time::Duration;

use crate::protocol::ch9329;
use crate::Result;

#[cfg(feature = "serial-backend")]
pub mod backend;

/// Primary baud rate the CH9329 is opened at.
pub const BAUD_PRIMARY: u32 = 115_200;

/// Fallback baud rate used when the chip doesn't respond at [`BAUD_PRIMARY`].
pub const BAUD_FALLBACK: u32 = 9_600;

/// A raw, byte-oriented serial link.
///
/// `Send` is required so the transport can be owned by a dedicated writer
/// thread (the pacing scheduler runs off the input thread).
pub trait SerialTransport: Send {
    /// Writes the entire buffer, returning only once all bytes are flushed to
    /// the OS.
    fn write_all(&mut self, bytes: &[u8]) -> Result<()>;

    /// Reads up to `buf.len()` bytes, blocking at most `timeout`. Returns the
    /// number of bytes read (`0` on timeout with no data).
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize>;

    /// Reconfigures the line baud rate (used by the open-time fallback).
    fn set_baud_rate(&mut self, baud: u32) -> Result<()>;
}

/// Outcome of [`connect_with_fallback`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Connection {
    /// The baud rate the link settled on.
    pub baud: u32,
    /// `true` if the chip answered `GET_INFO` (target verified responsive).
    /// `false` means the link is open but the chip did not answer — which is
    /// harmless on firmware that ignores `GET_INFO`.
    pub target_responsive: bool,
}

/// Negotiates the CH9329 link: probe `GET_INFO` at 115200, then 9600.
///
/// - If the chip answers at either rate, returns that rate with
///   `target_responsive = true`.
/// - If neither answers, returns the **primary** rate with
///   `target_responsive = false` (the chip likely ignores `GET_INFO`; the link
///   is still usable for input).
///
/// `probe_timeout` bounds each read attempt.
pub fn connect_with_fallback<T: SerialTransport>(
    transport: &mut T,
    probe_timeout: Duration,
) -> Result<Connection> {
    for baud in [BAUD_PRIMARY, BAUD_FALLBACK] {
        transport.set_baud_rate(baud)?;
        if probe_get_info(transport, probe_timeout)? {
            return Ok(Connection {
                baud,
                target_responsive: true,
            });
        }
    }
    // No response at either rate: default to the primary rate and proceed.
    transport.set_baud_rate(BAUD_PRIMARY)?;
    Ok(Connection {
        baud: BAUD_PRIMARY,
        target_responsive: false,
    })
}

/// Sends `GET_INFO` and returns `true` if a valid CH9329 response arrives
/// within `timeout`.
fn probe_get_info<T: SerialTransport>(transport: &mut T, timeout: Duration) -> Result<bool> {
    transport.write_all(&ch9329::get_info())?;
    let mut buf = [0u8; 64];
    let n = transport.read(&mut buf, timeout)?;
    Ok(ch9329::parse_response(&buf[..n]).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A scriptable transport: records writes, replays queued reads, and can be
    /// told which baud rates "respond" to GET_INFO.
    struct ScriptedSerial {
        baud: u32,
        responsive_bauds: Vec<u32>,
        pending: VecDeque<u8>,
        writes: usize,
    }

    impl ScriptedSerial {
        fn new(responsive_bauds: Vec<u32>) -> Self {
            Self {
                baud: 0,
                responsive_bauds,
                pending: VecDeque::new(),
                writes: 0,
            }
        }
    }

    impl SerialTransport for ScriptedSerial {
        fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
            self.writes += 1;
            // If the current baud is "responsive" and this is a GET_INFO,
            // queue a valid response for the next read.
            if self.responsive_bauds.contains(&self.baud) && bytes == ch9329::get_info().as_slice()
            {
                let resp = ch9329::frame(0x81, &[0x01]);
                self.pending.extend(resp);
            }
            Ok(())
        }

        fn read(&mut self, buf: &mut [u8], _timeout: Duration) -> Result<usize> {
            let n = buf.len().min(self.pending.len());
            for slot in buf.iter_mut().take(n) {
                *slot = self.pending.pop_front().unwrap();
            }
            Ok(n)
        }

        fn set_baud_rate(&mut self, baud: u32) -> Result<()> {
            self.baud = baud;
            Ok(())
        }
    }

    #[test]
    fn connects_at_primary_when_responsive() {
        let mut s = ScriptedSerial::new(vec![BAUD_PRIMARY]);
        let c = connect_with_fallback(&mut s, Duration::from_millis(10)).unwrap();
        assert_eq!(c.baud, BAUD_PRIMARY);
        assert!(c.target_responsive);
    }

    #[test]
    fn falls_back_to_9600() {
        let mut s = ScriptedSerial::new(vec![BAUD_FALLBACK]);
        let c = connect_with_fallback(&mut s, Duration::from_millis(10)).unwrap();
        assert_eq!(c.baud, BAUD_FALLBACK);
        assert!(c.target_responsive);
    }

    #[test]
    fn silent_chip_defaults_to_primary_unverified() {
        // No baud responds (firmware ignores GET_INFO).
        let mut s = ScriptedSerial::new(vec![]);
        let c = connect_with_fallback(&mut s, Duration::from_millis(10)).unwrap();
        assert_eq!(c.baud, BAUD_PRIMARY);
        assert!(!c.target_responsive);
    }
}
