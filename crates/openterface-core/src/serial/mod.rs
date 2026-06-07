//! Byte-level serial transport contract for the CH9329 link.
//!
//! The contract is deliberately raw (open/read/write/baud). CH9329 *framing*
//! lives in [`crate::protocol::ch9329`] and the *pacing* of writes lives in
//! [`crate::pacing`]; this trait only moves bytes. Implementations: a real
//! `serialport`-backed transport, a Linux PTY (for hardware-free integration
//! tests), and an in-memory mock in `openterface-test-support`.

use std::time::Duration;

use crate::Result;

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
