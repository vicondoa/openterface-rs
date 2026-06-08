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

    /// Sets the RTS (Request-To-Send) modem control line. Used by the CH9329
    /// factory-reset sequence (RTS pulse). Defaults to a no-op so test doubles
    /// and the PTY need not implement it; the real serial backend overrides it.
    fn set_rts(&mut self, _level: bool) -> Result<()> {
        Ok(())
    }
}

impl SerialTransport for Box<dyn SerialTransport> {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        (**self).write_all(bytes)
    }
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        (**self).read(buf, timeout)
    }
    fn set_baud_rate(&mut self, baud: u32) -> Result<()> {
        (**self).set_baud_rate(baud)
    }
    fn set_rts(&mut self, level: bool) -> Result<()> {
        (**self).set_rts(level)
    }
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

/// RTS hold time for a CH9329 factory reset (the C++ pulses RTS high ~4 s).
pub const FACTORY_RESET_RTS_HOLD: Duration = Duration::from_secs(4);

/// Settle time after releasing RTS before the software reconfigure (C++ ~500 ms).
pub const FACTORY_RESET_SETTLE: Duration = Duration::from_millis(500);

/// Extra wait after the RTS pulse before the software reconfigure (C++ ~1 s).
pub const FACTORY_RESET_POST_SETTLE: Duration = Duration::from_secs(1);

/// `resetChip` delays (C++): 100 ms after the first reset, 50 ms after the
/// config, 200 ms for the chip to restart after the final reset.
const RESET_CHIP_AFTER_RESET: Duration = Duration::from_millis(100);
const RESET_CHIP_AFTER_CFG: Duration = Duration::from_millis(50);
const RESET_CHIP_AFTER_FINAL: Duration = Duration::from_millis(200);

/// Sends the CH9329 software/HID reset command (`CMD 0x0F`) on `transport`.
///
/// This is the lightweight "reset HID" operation (C++ `Serial::resetHID`): it
/// re-initializes the chip's HID state without the RTS power-cycle.
pub fn reset_hid<T: SerialTransport>(transport: &mut T) -> Result<()> {
    transport.write_all(&ch9329::software_reset())
}

/// Resets and reconfigures the CH9329 to **mode 0x82 / 115200** (C++
/// `resetChip`): software reset, [`ch9329::set_para_cfg`], then a final reset to
/// apply, with the same inter-step delays. `sleep` is injected for tests.
pub fn reset_chip<T, S>(transport: &mut T, mut sleep: S) -> Result<()>
where
    T: SerialTransport,
    S: FnMut(Duration),
{
    transport.write_all(&ch9329::software_reset())?;
    sleep(RESET_CHIP_AFTER_RESET);
    transport.write_all(&ch9329::set_para_cfg())?;
    sleep(RESET_CHIP_AFTER_CFG);
    transport.write_all(&ch9329::software_reset())?;
    sleep(RESET_CHIP_AFTER_FINAL);
    Ok(())
}

/// Performs a full CH9329 **factory reset** (C++ `factoryReset`): pulses RTS
/// high for `rts_hold` (hardware reset), releases it, waits `settle` then an
/// extra [`FACTORY_RESET_POST_SETTLE`], and finally runs [`reset_chip`] to
/// reconfigure the chip to mode 0x82 / 115200. The `sleep` function is injected
/// so tests run without real delays; production passes [`std::thread::sleep`].
///
/// Use [`FACTORY_RESET_RTS_HOLD`] / [`FACTORY_RESET_SETTLE`] for the standard
/// durations.
pub fn factory_reset<T, S>(
    transport: &mut T,
    rts_hold: Duration,
    settle: Duration,
    mut sleep: S,
) -> Result<()>
where
    T: SerialTransport,
    S: FnMut(Duration),
{
    transport.set_rts(true)?;
    sleep(rts_hold);
    transport.set_rts(false)?;
    sleep(settle);
    sleep(FACTORY_RESET_POST_SETTLE);
    reset_chip(transport, &mut sleep)?;
    Ok(())
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
        written: Vec<Vec<u8>>,
        rts_log: Vec<bool>,
    }

    impl ScriptedSerial {
        fn new(responsive_bauds: Vec<u32>) -> Self {
            Self {
                baud: 0,
                responsive_bauds,
                pending: VecDeque::new(),
                writes: 0,
                written: Vec::new(),
                rts_log: Vec::new(),
            }
        }
    }

    impl SerialTransport for ScriptedSerial {
        fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
            self.writes += 1;
            self.written.push(bytes.to_vec());
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

        fn set_rts(&mut self, level: bool) -> Result<()> {
            self.rts_log.push(level);
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

    #[test]
    fn reset_hid_sends_software_reset() {
        let mut s = ScriptedSerial::new(vec![]);
        reset_hid(&mut s).unwrap();
        assert_eq!(s.written, vec![ch9329::software_reset()]);
    }

    #[test]
    fn factory_reset_pulses_rts_then_resets() {
        let mut s = ScriptedSerial::new(vec![]);
        let mut slept: Vec<Duration> = Vec::new();
        // Inject a recording sleeper so the test does not wait the real delays.
        factory_reset(&mut s, FACTORY_RESET_RTS_HOLD, FACTORY_RESET_SETTLE, |d| {
            slept.push(d)
        })
        .unwrap();
        // RTS goes high then low.
        assert_eq!(s.rts_log, vec![true, false]);
        // Delays: RTS hold, settle, post-settle, then resetChip's 100/50/200 ms.
        assert_eq!(
            slept,
            vec![
                FACTORY_RESET_RTS_HOLD,
                FACTORY_RESET_SETTLE,
                FACTORY_RESET_POST_SETTLE,
                RESET_CHIP_AFTER_RESET,
                RESET_CHIP_AFTER_CFG,
                RESET_CHIP_AFTER_FINAL,
            ]
        );
        // resetChip writes: reset, set_para_cfg, reset (reconfigure to mode 0x82).
        assert_eq!(
            s.written,
            vec![
                ch9329::software_reset(),
                ch9329::set_para_cfg(),
                ch9329::software_reset(),
            ]
        );
    }

    #[test]
    fn reset_chip_reconfigures_mode_82() {
        let mut s = ScriptedSerial::new(vec![]);
        reset_chip(&mut s, |_| {}).unwrap();
        assert_eq!(
            s.written,
            vec![
                ch9329::software_reset(),
                ch9329::set_para_cfg(),
                ch9329::software_reset(),
            ]
        );
    }
}
