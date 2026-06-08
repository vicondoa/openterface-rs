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
use openterface_core::video::{
    CaptureConfig, ColorRange, ColorSpace, FormatDesc, Frame, PixelFormat, VideoSource,
};
use openterface_core::{Error, Result};

/// A real, decode-valid 16x16 baseline JPEG (shared with the decode tests) used
/// for healthy simulated MJPEG frames.
const GRAY16_JPEG: &[u8] = include_bytes!("../fixtures/gray16.jpg");

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

/// A scripted fault the [`SimulatedVideoSource`] can inject, to exercise the
/// failure modes a real capture device exhibits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VideoFault {
    /// The next `next_frame` returns a timeout (no data within the deadline).
    Timeout,
    /// The next frame's payload is corrupt (e.g. truncated MJPEG).
    CorruptFrame,
    /// The device disappears mid-session: this and all later calls error
    /// (simulates a USB/IP detach).
    Disconnect,
}

/// A video source that emits synthetic frames on demand, with optional scripted
/// faults.
///
/// The default frame is a tiny solid-color buffer in the configured format —
/// enough to drive pipeline plumbing without a real capture device. Faults are
/// queued via [`SimulatedVideoSource::inject`] and consumed in order, one per
/// `next_frame` call.
#[derive(Default)]
pub struct SimulatedVideoSource {
    config: CaptureConfig,
    started: bool,
    frames: u64,
    faults: VecDeque<VideoFault>,
    disconnected: bool,
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
            ..Self::default()
        }
    }

    /// Queues a fault to be applied on an upcoming `next_frame` call.
    pub fn inject(&mut self, fault: VideoFault) {
        self.faults.push_back(fault);
    }

    fn synth_frame(&self, corrupt: bool) -> Frame {
        let (data, bytes_per_line) = match (self.config.format, corrupt) {
            // A real, decode-valid JPEG (the same 16x16 fixture the decode
            // tests use) so a healthy simulated frame round-trips through decode.
            (PixelFormat::Mjpeg, false) => (GRAY16_JPEG.to_vec(), 0),
            // Truncated JPEG: valid SOI, no EOI/scan → decode fails.
            (PixelFormat::Mjpeg, true) => (vec![0xFF, 0xD8], 0),
            // A full-size mid-gray YUYV buffer for the configured dimensions.
            (PixelFormat::Yuyv, false) => {
                let row = self.config.width as usize * 2;
                let mut buf = vec![0u8; row * self.config.height as usize];
                for px in buf.chunks_exact_mut(2) {
                    px[0] = 0x7F; // Y
                    px[1] = 0x80; // U/V
                }
                (buf, self.config.width * 2)
            }
            // A YUYV frame too short for its declared dimensions → decode fails.
            (PixelFormat::Yuyv, true) => (vec![0x00], self.config.width * 2),
        };
        // For MJPEG, report the fixture's intrinsic 16x16 so a decoded frame
        // matches; for YUYV, report the configured dimensions.
        let (width, height) = match self.config.format {
            PixelFormat::Mjpeg if !corrupt => (16, 16),
            _ => (self.config.width, self.config.height),
        };
        Frame {
            format: self.config.format,
            width,
            height,
            bytes_per_line,
            color_range: ColorRange::Limited,
            color_space: if height >= 720 {
                ColorSpace::Bt709
            } else {
                ColorSpace::Bt601
            },
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
        if self.disconnected {
            return Err(Error::Video("device disconnected".to_string()));
        }
        if !self.started {
            return Err(Error::Video("capture not started".to_string()));
        }
        match self.faults.pop_front() {
            Some(VideoFault::Timeout) => Err(Error::Timeout),
            Some(VideoFault::Disconnect) => {
                self.disconnected = true;
                Err(Error::Video("device disconnected".to_string()))
            }
            Some(VideoFault::CorruptFrame) => {
                self.frames += 1;
                Ok(self.synth_frame(true))
            }
            None => {
                self.frames += 1;
                Ok(self.synth_frame(false))
            }
        }
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
    fn simulated_video_injects_faults_in_order() {
        use openterface_core::decode::decode_frame;
        let mut v = SimulatedVideoSource::new();
        v.start().unwrap();
        v.inject(VideoFault::Timeout);
        v.inject(VideoFault::CorruptFrame);
        // First call: timeout.
        assert!(matches!(
            v.next_frame(Duration::from_millis(0)),
            Err(openterface_core::Error::Timeout)
        ));
        // Second call: a corrupt (truncated) frame is returned but fails decode.
        let corrupt = v.next_frame(Duration::from_millis(0)).unwrap();
        assert!(decode_frame(&corrupt).is_err());
        // Third call: a healthy frame that decodes successfully.
        let ok = v.next_frame(Duration::from_millis(0)).unwrap();
        assert!(decode_frame(&ok).is_ok());
    }

    #[test]
    fn healthy_simulated_frames_decode() {
        use openterface_core::decode::decode_frame;
        use openterface_core::video::{CaptureConfig, PixelFormat};
        // MJPEG.
        let mut v = SimulatedVideoSource::new();
        v.start().unwrap();
        let mjpeg = v.next_frame(Duration::from_millis(0)).unwrap();
        assert!(decode_frame(&mjpeg).is_ok());
        // YUYV at a small even size.
        let mut y = SimulatedVideoSource::with_config(CaptureConfig {
            width: 8,
            height: 4,
            fps: 30,
            format: PixelFormat::Yuyv,
        });
        y.start().unwrap();
        let yuyv = y.next_frame(Duration::from_millis(0)).unwrap();
        let img = decode_frame(&yuyv).unwrap();
        assert_eq!((img.width, img.height), (8, 4));
    }

    #[test]
    fn simulated_video_disconnect_is_sticky() {
        let mut v = SimulatedVideoSource::new();
        v.start().unwrap();
        v.inject(VideoFault::Disconnect);
        assert!(v.next_frame(Duration::from_millis(0)).is_err());
        // Stays disconnected on subsequent calls.
        assert!(v.next_frame(Duration::from_millis(0)).is_err());
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
