//! Real V4L2 capture backend using the `v4l` crate.
//!
//! Gated behind the `video-backend` feature so the default (no-hardware) test
//! build does not pull in `v4l`/`libv4l`. Real-device validation happens on the
//! work-ssd VM (see `docs/explanation/v4l2-spike.md`); the deterministic
//! pipeline tests use the `SimulatedVideoSource` in `openterface-test-support`.

use std::time::Duration;

use v4l::buffer::Type;
use v4l::format::{Colorspace, Quantization};
use v4l::frameinterval::FrameIntervalEnum;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::{Device, Format, FourCC, Fraction, FrameInterval};

use crate::video::{
    integer_fps_from_interval, v4l2_format_candidates, CaptureConfig, CaptureFormatCandidate,
    ColorRange, ColorSpace, FormatDesc, Frame, PixelFormat, VideoSource,
};
use crate::{Error, Result};

/// FourCC for Motion-JPEG.
const FOURCC_MJPG: &[u8; 4] = b"MJPG";
/// FourCC for packed YUYV 4:2:2.
const FOURCC_YUYV: &[u8; 4] = b"YUYV";

fn fourcc(format: PixelFormat) -> FourCC {
    match format {
        PixelFormat::Mjpeg => FourCC::new(FOURCC_MJPG),
        PixelFormat::Yuyv => FourCC::new(FOURCC_YUYV),
    }
}

fn pixel_format(repr: &[u8; 4]) -> Option<PixelFormat> {
    match repr {
        b if b == FOURCC_MJPG => Some(PixelFormat::Mjpeg),
        b if b == FOURCC_YUYV => Some(PixelFormat::Yuyv),
        _ => None,
    }
}

/// Maps a V4L2 colorspace to our conversion matrix selector.
fn map_colorspace(cs: Colorspace) -> ColorSpace {
    match cs {
        Colorspace::Rec709 => ColorSpace::Bt709,
        Colorspace::SMPTE170M | Colorspace::NTSC | Colorspace::SMPTE240M => ColorSpace::Bt601,
        // JPEG/sRGB carry full-range BT.601 primaries in practice.
        Colorspace::JPEG | Colorspace::SRGB => ColorSpace::Bt601,
        _ => ColorSpace::Unknown,
    }
}

/// Maps a V4L2 quantization (and format) to our range selector. `Default`
/// resolves per the usual conventions: encoded/JPEG is full range, packed YUYV
/// is limited range.
fn map_range(q: Quantization, format: PixelFormat) -> ColorRange {
    match q {
        Quantization::FullRange => ColorRange::Full,
        Quantization::LimitedRange => ColorRange::Limited,
        Quantization::Default => match format {
            PixelFormat::Mjpeg => ColorRange::Full,
            PixelFormat::Yuyv => ColorRange::Limited,
        },
    }
}

fn format_for_candidate(base: Format, candidate: CaptureFormatCandidate) -> Format {
    let mut fmt = base;
    fmt.width = candidate.width;
    fmt.height = candidate.height;
    fmt.fourcc = fourcc(candidate.format);
    fmt
}

fn format_matches_candidate(applied: &Format, candidate: CaptureFormatCandidate) -> bool {
    applied.width == candidate.width
        && applied.height == candidate.height
        && pixel_format(&applied.fourcc.repr) == Some(candidate.format)
}

fn push_frame_rate(rates: &mut Vec<u32>, interval: Fraction) {
    if let Some(fps) = integer_fps_from_interval(interval.numerator, interval.denominator) {
        if !rates.contains(&fps) {
            rates.push(fps);
        }
    }
}

fn frame_rates_from_intervals(intervals: Vec<FrameInterval>) -> Vec<u32> {
    let mut rates = Vec::new();
    for interval in intervals {
        match interval.interval {
            FrameIntervalEnum::Discrete(interval) => push_frame_rate(&mut rates, interval),
            FrameIntervalEnum::Stepwise(stepwise) => {
                push_frame_rate(&mut rates, stepwise.min);
                push_frame_rate(&mut rates, stepwise.max);
            }
        }
    }
    rates
}

/// A [`VideoSource`] backed by a real V4L2 device (e.g. `/dev/video2`).
///
/// Field order matters: `stream` is declared before `device` so it is dropped
/// first (the stream borrows the device). The device is boxed so its address is
/// stable even if the `V4l2Source` value is moved, which is what makes the
/// borrow extension in [`V4l2Source::start`] sound.
pub struct V4l2Source {
    stream: Option<Stream<'static>>,
    device: Box<Device>,
    active: Option<CaptureConfig>,
    /// The exact V4L2 format the driver applied (stride/colorspace/quantization).
    applied: Option<Format>,
}

impl V4l2Source {
    /// Opens the V4L2 device at `path`.
    pub fn open(path: &str) -> Result<Self> {
        let device =
            Device::with_path(path).map_err(|e| Error::Video(format!("open {path}: {e}")))?;
        Ok(Self {
            stream: None,
            device: Box::new(device),
            active: None,
            applied: None,
        })
    }
}

impl VideoSource for V4l2Source {
    fn supported_formats(&self) -> Result<Vec<FormatDesc>> {
        let mut out = Vec::new();
        let formats = Capture::enum_formats(&*self.device)
            .map_err(|e| Error::Video(format!("enum_formats: {e}")))?;
        for desc in formats {
            let Some(format) = pixel_format(&desc.fourcc.repr) else {
                continue;
            };
            let sizes = Capture::enum_framesizes(&*self.device, desc.fourcc)
                .map_err(|e| Error::Video(format!("enum_framesizes: {e}")))?;
            for size in sizes {
                for discrete in size.size.to_discrete() {
                    let intervals = Capture::enum_frameintervals(
                        &*self.device,
                        desc.fourcc,
                        discrete.width,
                        discrete.height,
                    )
                    .map_err(|e| Error::Video(format!("enum_frameintervals: {e}")))?;
                    out.push(FormatDesc {
                        format,
                        width: discrete.width,
                        height: discrete.height,
                        frame_rates: frame_rates_from_intervals(intervals),
                    });
                }
            }
        }
        Ok(out)
    }

    fn configure(&mut self, config: CaptureConfig) -> Result<()> {
        let current =
            Capture::format(&*self.device).map_err(|e| Error::Video(format!("get format: {e}")))?;
        let mut attempts = Vec::new();
        let mut selected = None;

        for candidate in v4l2_format_candidates(config) {
            let fmt = format_for_candidate(current, candidate);
            match Capture::set_format(&*self.device, &fmt) {
                Ok(applied) if format_matches_candidate(&applied, candidate) => {
                    selected = Some((candidate, applied));
                    break;
                }
                Ok(applied) => attempts.push(format!(
                    "{}x{} {:?}: driver adjusted to {}x{} {:?}",
                    candidate.width,
                    candidate.height,
                    candidate.format,
                    applied.width,
                    applied.height,
                    pixel_format(&applied.fourcc.repr)
                )),
                Err(e) => attempts.push(format!(
                    "{}x{} {:?}: {e}",
                    candidate.width, candidate.height, candidate.format
                )),
            }
        }

        let (candidate, applied) = selected.ok_or_else(|| {
            Error::Video(format!(
                "set format failed for all V4L2 candidates: {}",
                attempts.join("; ")
            ))
        })?;
        let fps = self.set_frame_rate(config.fps)?;
        self.active = Some(CaptureConfig {
            width: applied.width,
            height: applied.height,
            fps,
            format: candidate.format,
        });
        self.applied = Some(applied);
        Ok(())
    }

    fn active_config(&self) -> Option<CaptureConfig> {
        self.active
    }

    fn start(&mut self) -> Result<()> {
        self.stream = Some(self.make_stream()?);
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.stream = None;
        Ok(())
    }

    fn next_frame(&mut self, timeout: Duration) -> Result<Frame> {
        let applied = self
            .applied
            .ok_or_else(|| Error::Video("no active config".to_string()))?;
        let config = self
            .active
            .ok_or_else(|| Error::Video("no active config".to_string()))?;
        // `v4l`'s set_timeout does `as_millis().try_into::<i32>().unwrap()`, so
        // clamp to i32::MAX ms to avoid a panic on an absurdly large timeout.
        let timeout = timeout.min(Duration::from_millis(i32::MAX as u64));

        // Distinguish a (recoverable) timeout from a hard error so we can react
        // after releasing the mutable borrow of `self.stream`.
        enum Outcome {
            Frame(Box<Frame>),
            Timeout,
        }

        let outcome = {
            let stream = self
                .stream
                .as_mut()
                .ok_or_else(|| Error::Video("capture not started".to_string()))?;
            stream.set_timeout(timeout);
            match stream.next() {
                Ok((buf, meta)) => {
                    // Only the captured bytes are valid; the buffer may be larger.
                    let used = (meta.bytesused as usize).min(buf.len());
                    let data = buf[..used].to_vec();
                    let bytes_per_line = match config.format {
                        PixelFormat::Yuyv => applied.stride,
                        PixelFormat::Mjpeg => 0,
                    };
                    Outcome::Frame(Box::new(Frame {
                        format: config.format,
                        width: config.width,
                        height: config.height,
                        bytes_per_line,
                        color_range: map_range(applied.quantization, config.format),
                        color_space: map_colorspace(applied.colorspace),
                        timestamp: Duration::ZERO,
                        data,
                    }))
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    Outcome::Timeout
                }
                Err(e) => return Err(Error::Video(format!("dequeue: {e}"))),
            }
        };

        match outcome {
            Outcome::Frame(frame) => Ok(*frame),
            Outcome::Timeout => {
                // A timed-out dequeue leaves the v4l Stream's buffer bookkeeping
                // wedged (the next `next()` would re-queue an already-queued
                // buffer and fail persistently). Rebuild the stream so a later
                // call can recover cleanly.
                self.stream = None;
                self.stream = Some(self.make_stream()?);
                Err(Error::Timeout)
            }
        }
    }
}

impl V4l2Source {
    fn set_frame_rate(&self, fps: u32) -> Result<u32> {
        if fps == 0 {
            return Err(Error::Video("fps must be greater than zero".to_string()));
        }

        let mut params = Capture::params(&*self.device)
            .map_err(|e| Error::Video(format!("get stream parameters: {e}")))?;
        params.interval = Fraction::new(1, fps);
        let applied = Capture::set_params(&*self.device, &params)
            .map_err(|e| Error::Video(format!("set frame rate {fps}fps: {e}")))?;
        integer_fps_from_interval(applied.interval.numerator, applied.interval.denominator)
            .ok_or_else(|| {
                Error::Video(format!(
                    "device returned invalid frame interval {}/{}",
                    applied.interval.numerator, applied.interval.denominator
                ))
            })
    }

    /// Creates an mmap capture stream borrowing the boxed device.
    fn make_stream(&self) -> Result<Stream<'static>> {
        let stream = Stream::with_buffers(&self.device, Type::VideoCapture, 4)
            .map_err(|e| Error::Video(format!("start stream: {e}")))?;
        // SAFETY: the stream borrows `*self.device`. The device is heap-boxed,
        // so its address is stable across moves of `V4l2Source`, and `stream`
        // is declared before `device`, so it is dropped (ending the borrow)
        // before the device. Extending the borrow to 'static is therefore
        // sound for the lifetime of this value.
        Ok(unsafe { std::mem::transmute::<Stream<'_>, Stream<'static>>(stream) })
    }
}
