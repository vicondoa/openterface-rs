//! The idle MJPEG-decode throttle state machine (pure, always built).
//!
//! Decoding every frame of a static remote screen wastes CPU. This reproduces
//! the C++ fork's behavior (see `docs/explanation/wgpu-spike.md` and
//! `docs/reference/cpp-cli-behavior.md`):
//!
//! 1. **Raw dedup** — byte-identical encoded frames skip decode *and* upload.
//! 2. **Non-deterministic MJPEG** — after a decode, identical *decoded* pixels
//!    skip the GPU upload.
//! 3. **Idle gate** — after `idle_after_frames` static frames, cap decode
//!    attempts to one per `idle_decode`.
//! 4. **Input wake** — any input keeps full-rate decode for `input_wake`.
//! 5. **Watchdog** — re-present the cached frame at least every `watchdog`
//!    (anti-freeze), no decode required.
//! 6. **Disable** — `OPENTERFACE_THROTTLE=0` always decodes.
//!
//! Time is passed in explicitly, so the whole machine is deterministic and
//! tested with no clock.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

/// Environment variables controlling the throttle (defaults match the C++ fork).
pub const ENV_THROTTLE: &str = "OPENTERFACE_THROTTLE";
pub const ENV_IDLE_DECODE_MS: &str = "OPENTERFACE_IDLE_DECODE_MS";
pub const ENV_INPUT_WAKE_MS: &str = "OPENTERFACE_INPUT_WAKE_MS";
pub const ENV_IDLE_WATCHDOG_MS: &str = "OPENTERFACE_IDLE_WATCHDOG_MS";

/// Throttle configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThrottleConfig {
    /// When `false`, every frame is decoded and uploaded.
    pub enabled: bool,
    /// Minimum interval between decodes once idle.
    pub idle_decode: Duration,
    /// Full-rate window after any input.
    pub input_wake: Duration,
    /// Forced cached-frame refresh interval (anti-freeze).
    pub watchdog: Duration,
    /// Consecutive static frames before declaring idle.
    pub idle_after_frames: u32,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            idle_decode: Duration::from_millis(100),
            input_wake: Duration::from_millis(250),
            watchdog: Duration::from_millis(1000),
            idle_after_frames: 15,
        }
    }
}

impl ThrottleConfig {
    /// Reads the configuration from the `OPENTERFACE_*` environment, falling
    /// back to defaults for unset/invalid values.
    #[must_use]
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if std::env::var(ENV_THROTTLE).ok().as_deref() == Some("0") {
            cfg.enabled = false;
        }
        if let Some(ms) = env_ms(ENV_IDLE_DECODE_MS) {
            cfg.idle_decode = Duration::from_millis(ms);
        }
        if let Some(ms) = env_ms(ENV_INPUT_WAKE_MS) {
            cfg.input_wake = Duration::from_millis(ms);
        }
        if let Some(ms) = env_ms(ENV_IDLE_WATCHDOG_MS) {
            cfg.watchdog = Duration::from_millis(ms);
        }
        cfg
    }
}

fn env_ms(key: &str) -> Option<u64> {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&n| n <= 100_000)
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// The throttle's per-frame decode/upload/refresh decision machine.
///
/// All times are an opaque monotonic value supplied by the caller (the GUI uses
/// `Instant`; tests use a synthetic millisecond counter via [`Clock`]).
pub struct FrameThrottle {
    config: ThrottleConfig,
    last_raw_hash: Option<u64>,
    last_decoded_hash: Option<u64>,
    static_count: u32,
    last_input: Option<Duration>,
    last_decode: Option<Duration>,
    last_upload: Option<Duration>,
    last_refresh: Option<Duration>,
    /// Set once a frame has been decoded and displayed.
    have_displayed: bool,
    /// Whether byte-identical raw frames reliably mean identical pixels. Starts
    /// optimistic; cleared if a decoded-pixel compare ever shows raw-different
    /// frames decoding to the same image (non-deterministic MJPEG encoder).
    raw_reliable: bool,
}

impl FrameThrottle {
    /// Creates a throttle with the given configuration.
    #[must_use]
    pub fn new(config: ThrottleConfig) -> Self {
        Self {
            config,
            last_raw_hash: None,
            last_decoded_hash: None,
            static_count: 0,
            last_input: None,
            last_upload: None,
            last_decode: None,
            last_refresh: None,
            have_displayed: false,
            raw_reliable: true,
        }
    }

    /// Records input activity at `now` (opens the full-rate wake window).
    pub fn note_input(&mut self, now: Duration) {
        self.last_input = Some(now);
    }

    /// Records that the cached frame was re-presented at `now` (used to gate the
    /// anti-freeze watchdog so it fires once per interval, not every loop).
    pub fn note_refresh(&mut self, now: Duration) {
        self.last_refresh = Some(now);
    }

    /// Decides whether the raw `frame` should be decoded at `now`.
    pub fn should_decode(&mut self, now: Duration, frame: &[u8]) -> bool {
        if !self.config.enabled {
            self.last_decode = Some(now);
            return true;
        }
        // Within the input-wake window: always full rate. (We keep static_count
        // so idle-gating resumes immediately once the wake window closes.)
        if self.within_wake(now) {
            self.last_raw_hash = Some(hash_bytes(frame));
            self.last_decode = Some(now);
            return true;
        }
        let hash = hash_bytes(frame);
        if self.last_raw_hash == Some(hash) {
            // Byte-identical raw frame. Once we have displayed a frame and the
            // raw stream is deterministic, skip decode AND upload entirely.
            if self.have_displayed && self.raw_reliable {
                return false;
            }
            // Non-deterministic encoder: gate decode to the idle interval.
            self.static_count = self.static_count.saturating_add(1);
            if self.static_count < self.config.idle_after_frames {
                self.last_decode = Some(now);
                return true;
            }
            let due = match self.last_decode {
                None => true,
                Some(last) => now.saturating_sub(last) >= self.config.idle_decode,
            };
            if due {
                self.last_decode = Some(now);
            }
            return due;
        }
        // Raw bytes changed: a fresh frame, decode it.
        self.last_raw_hash = Some(hash);
        self.static_count = 0;
        self.last_decode = Some(now);
        true
    }

    /// After a decode, decides whether the decoded pixels should be uploaded at
    /// `now` (skips the upload when the decoded image is unchanged).
    pub fn should_upload(&mut self, now: Duration, decoded: &[u8]) -> bool {
        let hash = hash_bytes(decoded);
        if self.last_decoded_hash == Some(hash) {
            // We decoded (because raw differed or a gate elapsed) yet the pixels
            // are identical → the raw stream is non-deterministic. Stop trusting
            // raw dedup so future static frames are idle-gated, not fast-skipped.
            self.raw_reliable = false;
            return false;
        }
        self.last_decoded_hash = Some(hash);
        self.last_upload = Some(now);
        self.last_refresh = Some(now);
        self.have_displayed = true;
        true
    }

    /// Whether the cached frame should be re-presented now (anti-freeze
    /// watchdog) even though nothing changed. Gated by the last upload **or**
    /// refresh so it fires at most once per `watchdog` interval.
    #[must_use]
    pub fn should_force_refresh(&self, now: Duration) -> bool {
        if !self.config.enabled || !self.have_displayed {
            return false;
        }
        let base = match (self.last_refresh, self.last_upload) {
            (Some(r), Some(u)) => r.max(u),
            (Some(r), None) => r,
            (None, Some(u)) => u,
            (None, None) => return false,
        };
        now.saturating_sub(base) >= self.config.watchdog
    }

    fn within_wake(&self, now: Duration) -> bool {
        match self.last_input {
            None => false,
            Some(t) => now.saturating_sub(t) < self.config.input_wake,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ThrottleConfig {
        ThrottleConfig::default()
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn changing_frames_always_decode() {
        let mut t = FrameThrottle::new(cfg());
        assert!(t.should_decode(ms(0), b"frame-a"));
        assert!(t.should_decode(ms(33), b"frame-b"));
        assert!(t.should_decode(ms(66), b"frame-c"));
    }

    #[test]
    fn static_frames_skip_decode_once_displayed() {
        let mut t = FrameThrottle::new(cfg());
        // First frame: decode + upload (now "displayed").
        assert!(t.should_decode(ms(0), b"static"));
        assert!(t.should_upload(ms(0), b"pixels"));
        // Byte-identical raw frames now skip decode entirely (deterministic).
        assert!(!t.should_decode(ms(33), b"static"));
        assert!(!t.should_decode(ms(66), b"static"));
    }

    #[test]
    fn nondeterministic_stream_falls_back_to_idle_gate() {
        let mut t = FrameThrottle::new(cfg());
        // raw "a" → pixels P.
        assert!(t.should_decode(ms(0), b"raw-a"));
        assert!(t.should_upload(ms(0), b"P"));
        // raw "b" (different bytes) → same pixels P: marks the stream
        // non-deterministic and skips the upload.
        assert!(t.should_decode(ms(33), b"raw-b"));
        assert!(!t.should_upload(ms(33), b"P"));
        // Now byte-identical raw frames are idle-gated (decoded) rather than
        // fast-skipped, since raw dedup is no longer trusted.
        for i in 1..15 {
            assert!(t.should_decode(ms(33 + i * 33), b"raw-b"));
        }
    }

    #[test]
    fn input_wake_forces_full_rate() {
        let mut t = FrameThrottle::new(cfg());
        for i in 0..20 {
            t.should_decode(ms(i * 33), b"static"); // drive into idle
        }
        let base = 20 * 33;
        t.note_input(ms(base));
        // Within the 250ms wake window, even static frames decode.
        assert!(t.should_decode(ms(base + 10), b"static"));
        assert!(t.should_decode(ms(base + 240), b"static"));
        // After the wake window, gating resumes.
        assert!(!t.should_decode(ms(base + 300), b"static"));
    }

    #[test]
    fn disabled_always_decodes() {
        let mut t = FrameThrottle::new(ThrottleConfig {
            enabled: false,
            ..cfg()
        });
        for i in 0..50 {
            assert!(t.should_decode(ms(i), b"static"));
        }
        assert!(!t.should_force_refresh(ms(100_000)));
    }

    #[test]
    fn upload_skipped_when_decoded_unchanged() {
        let mut t = FrameThrottle::new(cfg());
        assert!(t.should_upload(ms(0), b"pixels-1"));
        assert!(!t.should_upload(ms(33), b"pixels-1")); // identical → skip
        assert!(t.should_upload(ms(66), b"pixels-2")); // changed → upload
    }

    #[test]
    fn watchdog_forces_refresh_after_interval_and_debounces() {
        let mut t = FrameThrottle::new(cfg());
        t.should_decode(ms(0), b"raw");
        t.should_upload(ms(0), b"pixels"); // displayed at t=0
        assert!(!t.should_force_refresh(ms(500)));
        assert!(t.should_force_refresh(ms(1000))); // watchdog = 1000ms
                                                   // The GUI records the refresh; the next forced refresh must wait another
                                                   // full interval (no continuous redraw spin under ControlFlow::Poll).
        t.note_refresh(ms(1000));
        assert!(!t.should_force_refresh(ms(1500)));
        assert!(t.should_force_refresh(ms(2000)));
    }
}
