//! Real OS-backed [`SerialTransport`] using the `serialport` crate.
//!
//! Gated behind the `serial-backend` feature so the default (no-hardware) test
//! build does not pull in `serialport`/`libudev`. The pure negotiation logic in
//! [`super::connect_with_fallback`] drives this transport at runtime.

use std::time::Duration;

use crate::serial::{SerialTransport, BAUD_PRIMARY};
use crate::{Error, Result};

/// A [`SerialTransport`] backed by a real serial port (e.g. `/dev/ttyACM0`).
pub struct SerialPortTransport {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialPortTransport {
    /// Opens `path` at [`BAUD_PRIMARY`] with 8N1 framing.
    pub fn open(path: &str) -> Result<Self> {
        let port = serialport::new(path, BAUD_PRIMARY)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| Error::Transport(format!("open {path}: {e}")))?;
        Ok(Self { port })
    }
}

impl SerialTransport for SerialPortTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        self.port
            .write_all(bytes)
            .map_err(|e| Error::Transport(format!("write: {e}")))?;
        self.port
            .flush()
            .map_err(|e| Error::Transport(format!("flush: {e}")))
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        use std::io::Read;
        self.port
            .set_timeout(timeout)
            .map_err(|e| Error::Transport(format!("set_timeout: {e}")))?;
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            // A read timeout with no data is not an error for our probe loop.
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(e) => Err(Error::Transport(format!("read: {e}"))),
        }
    }

    fn set_baud_rate(&mut self, baud: u32) -> Result<()> {
        self.port
            .set_baud_rate(baud)
            .map_err(|e| Error::Transport(format!("set_baud_rate({baud}): {e}")))
    }

    fn set_rts(&mut self, level: bool) -> Result<()> {
        self.port
            .write_request_to_send(level)
            .map_err(|e| Error::Transport(format!("set_rts({level}): {e}")))
    }
}
