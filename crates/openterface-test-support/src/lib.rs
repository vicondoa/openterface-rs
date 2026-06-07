//! Test doubles and fixtures for `openterface-core`.
//!
//! These let the full pipeline be exercised with **no hardware**. W2 extends
//! them with fault injection (dropped/slow/partial writes, corrupt/delayed
//! frames, mid-session disappearance); W0 provides functional happy-path
//! doubles for [`SerialTransport`], [`VideoSource`], and [`DeviceScanner`].

use std::collections::VecDeque;
use std::time::Duration;

use openterface_core::discovery::{DeviceInfo, DeviceScanner};
use openterface_core::serial::{SerialTransport, BAUD_PRIMARY};
use openterface_core::video::{CaptureConfig, FormatDesc, Frame, PixelFormat, VideoSource};
use openterface_core::{Error, Result};

/// In-memory serial transport: records everything written, replays scripted
/// reads. The recorded bytes are the basis for protocol-level assertions.
#[derive(Default)]
pub struct MockSerial {
    /// All bytes written, in order.
    written: Vec<u8>,
    /// Bytes to hand back from `read`, in order.
    to_read: VecDeque<u8>,
    /// Last baud rate set via `set_baud_rate`.
    baud: u32,
}

impl MockSerial {
    /// Creates an empty mock at the primary baud rate.
    #[must_use]
    pub fn new() -> Self {
        Self {
            written: Vec::new(),
            to_read: VecDeque::new(),
            baud: BAUD_PRIMARY,
        }
    }

    /// Queues bytes to be returned by future `read` calls.
    pub fn push_readable(&mut self, bytes: &[u8]) {
        self.to_read.extend(bytes.iter().copied());
    }

    /// Returns everything written so far.
    #[must_use]
    pub fn written(&self) -> &[u8] {
        &self.written
    }

    /// Returns the last baud rate set.
    #[must_use]
    pub fn baud(&self) -> u32 {
        self.baud
    }
}

impl SerialTransport for MockSerial {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
        self.written.extend_from_slice(bytes);
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], _timeout: Duration) -> Result<usize> {
        let n = buf.len().min(self.to_read.len());
        for slot in buf.iter_mut().take(n) {
            *slot = self.to_read.pop_front().expect("checked len");
        }
        Ok(n)
    }

    fn set_baud_rate(&mut self, baud: u32) -> Result<()> {
        self.baud = baud;
        Ok(())
    }
}

/// A video source that emits a fixed synthetic frame on demand.
///
/// The default frame is a tiny solid-color buffer in the configured format —
/// enough to drive pipeline plumbing without a real capture device.
#[derive(Default)]
pub struct SimulatedVideoSource {
    config: CaptureConfig,
    started: bool,
    frames: u64,
}

impl SimulatedVideoSource {
    /// Creates a simulated source with the default capture configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a simulated source with a specific configuration.
    #[must_use]
    pub fn with_config(config: CaptureConfig) -> Self {
        Self {
            config,
            started: false,
            frames: 0,
        }
    }

    fn synth_frame(&self) -> Frame {
        // A trivially small placeholder payload; W2.3/W2.5 replace this with
        // real synthetic MJPEG/YUYV bytes derived from W1.2 fixtures.
        let data = match self.config.format {
            PixelFormat::Mjpeg => vec![0xFF, 0xD8, 0xFF, 0xD9], // empty JPEG SOI/EOI
            PixelFormat::Yuyv => vec![0x00, 0x80, 0x00, 0x80],
        };
        Frame {
            format: self.config.format,
            width: self.config.width,
            height: self.config.height,
            timestamp: Duration::from_millis(self.frames * 33),
            data,
        }
    }
}

impl VideoSource for SimulatedVideoSource {
    fn supported_formats(&self) -> Result<Vec<FormatDesc>> {
        Ok(vec![FormatDesc {
            format: PixelFormat::Mjpeg,
            width: 1920,
            height: 1080,
            frame_rates: vec![30],
        }])
    }

    fn configure(&mut self, config: CaptureConfig) -> Result<()> {
        self.config = config;
        Ok(())
    }

    fn active_config(&self) -> Option<CaptureConfig> {
        Some(self.config)
    }

    fn start(&mut self) -> Result<()> {
        self.started = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.started = false;
        Ok(())
    }

    fn next_frame(&mut self, _timeout: Duration) -> Result<Frame> {
        if !self.started {
            return Err(Error::Video("capture not started".to_string()));
        }
        self.frames += 1;
        Ok(self.synth_frame())
    }
}

/// A scanner that returns a fixed list of devices (e.g. parsed from a sample
/// sysfs fixture tree in W2.6).
#[derive(Default)]
pub struct FixtureScanner {
    /// Devices this scanner reports.
    pub devices: Vec<DeviceInfo>,
}

impl FixtureScanner {
    /// Creates a scanner reporting the given devices.
    #[must_use]
    pub fn new(devices: Vec<DeviceInfo>) -> Self {
        Self { devices }
    }
}

impl DeviceScanner for FixtureScanner {
    fn scan(&self) -> Result<Vec<DeviceInfo>> {
        Ok(self.devices.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_serial_records_and_replays() {
        let mut s = MockSerial::new();
        s.write_all(&[0x57, 0xAB]).unwrap();
        s.push_readable(&[0x01, 0x02]);
        let mut buf = [0u8; 4];
        let n = s.read(&mut buf, Duration::from_millis(0)).unwrap();
        assert_eq!(n, 2);
        assert_eq!(&buf[..2], &[0x01, 0x02]);
        assert_eq!(s.written(), &[0x57, 0xAB]);
    }

    #[test]
    fn simulated_video_needs_start() {
        let mut v = SimulatedVideoSource::new();
        assert!(v.next_frame(Duration::from_millis(0)).is_err());
        v.start().unwrap();
        let f = v.next_frame(Duration::from_millis(0)).unwrap();
        assert_eq!(f.format, PixelFormat::Mjpeg);
    }

    #[test]
    fn fixture_scanner_returns_devices() {
        let scanner = FixtureScanner::new(vec![DeviceInfo {
            description: "test".to_string(),
            ..Default::default()
        }]);
        assert_eq!(scanner.scan().unwrap().len(), 1);
    }
}
