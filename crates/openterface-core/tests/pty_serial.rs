//! PTY round-trip integration test for the serial negotiation logic.
//!
//! Exercises [`openterface_core::serial::connect_with_fallback`] over a **real
//! kernel pseudo-terminal** (no hardware): a background "device" thread reads
//! the slave end and answers `GET_INFO` with a valid CH9329 response, while the
//! negotiation runs against a transport wrapping the master end. This validates
//! the actual byte read/write path, not just a mock.

use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use nix::pty::openpty;

use openterface_core::protocol::ch9329;
use openterface_core::serial::{connect_with_fallback, SerialTransport, BAUD_PRIMARY};
use openterface_core::Result;

/// A `SerialTransport` over a raw fd (the PTY master).
struct FdTransport {
    fd: OwnedFd,
}

impl SerialTransport for FdTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        let mut file = unsafe { fd_as_file(self.fd.as_raw_fd()) };
        file.write_all(bytes).unwrap();
        std::mem::forget(file); // don't close the borrowed fd
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        // The PTY is opened non-blocking-ish via a short poll loop.
        let start = std::time::Instant::now();
        loop {
            let mut file = unsafe { fd_as_file(self.fd.as_raw_fd()) };
            let res = file.read(buf);
            std::mem::forget(file);
            match res {
                Ok(n) if n > 0 => return Ok(n),
                _ => {
                    if start.elapsed() >= timeout {
                        return Ok(0);
                    }
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }

    fn set_baud_rate(&mut self, _baud: u32) -> Result<()> {
        // A PTY has no real baud rate; the negotiation logic still drives this.
        Ok(())
    }
}

unsafe fn fd_as_file(raw: std::os::fd::RawFd) -> std::fs::File {
    use std::os::fd::FromRawFd;
    unsafe { std::fs::File::from_raw_fd(raw) }
}

#[test]
fn connect_negotiates_over_real_pty() {
    let pty = openpty(None, None).expect("openpty");
    let master: OwnedFd = pty.master;
    let slave: OwnedFd = pty.slave;

    // Put the line discipline in raw mode on both ends: a PTY defaults to
    // canonical mode with echo, which would mangle/echo our binary CH9329
    // frames (CR/LF translation, etc.).
    for fd in [master.as_fd(), slave.as_fd()] {
        let mut t = nix::sys::termios::tcgetattr(fd).unwrap();
        nix::sys::termios::cfmakeraw(&mut t);
        nix::sys::termios::tcsetattr(fd, nix::sys::termios::SetArg::TCSANOW, &t).unwrap();
    }

    // Make both ends non-blocking so neither the negotiation read nor the
    // device thread can block forever (the device thread must be able to see
    // the stop signal between polls).
    for fd in [master.as_raw_fd(), slave.as_raw_fd()] {
        let flags = nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_GETFL).unwrap();
        let mut oflags = nix::fcntl::OFlag::from_bits_truncate(flags);
        oflags.insert(nix::fcntl::OFlag::O_NONBLOCK);
        nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_SETFL(oflags)).unwrap();
    }

    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    // "Device" thread: read the slave, answer GET_INFO with a valid response.
    let device = thread::spawn(move || {
        let mut file = unsafe { fd_as_file(slave.as_raw_fd()) };
        let get_info = ch9329::get_info();
        let mut acc: Vec<u8> = Vec::new();
        let mut buf = [0u8; 64];
        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }
            match file.read(&mut buf) {
                Ok(n) if n > 0 => {
                    acc.extend_from_slice(&buf[..n]);
                    if acc
                        .windows(get_info.len())
                        .any(|w| w == get_info.as_slice())
                    {
                        let resp = ch9329::frame(0x81, &[0x01]);
                        file.write_all(&resp).unwrap();
                        acc.clear();
                    }
                }
                _ => thread::sleep(Duration::from_millis(2)),
            }
        }
        std::mem::forget(file);
        // keep slave open until thread end
        drop(slave);
    });

    let mut transport = FdTransport { fd: master };
    let conn = connect_with_fallback(&mut transport, Duration::from_millis(500)).unwrap();

    assert_eq!(conn.baud, BAUD_PRIMARY);
    assert!(
        conn.target_responsive,
        "device answered GET_INFO over the PTY, so the chip should be verified responsive"
    );

    let _ = stop_tx.send(());
    let _ = device.join();
}
