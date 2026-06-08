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
        }
    }

    /// Records input activity at `now` (opens the full-rate wake window).
    pub fn note_input(&mut self, now: Duration) {
        self.last_input = Some(now);
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
            self.static_count = self.static_count.saturating_add(1);
            // Below the idle threshold we keep decoding; once idle, gate it.
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
            return false;
        }
        self.last_decoded_hash = Some(hash);
        self.last_upload = Some(now);
        true
    }

    /// Whether the cached frame should be re-presented now (anti-freeze
    /// watchdog) even though nothing changed.
    #[must_use]
    pub fn should_force_refresh(&self, now: Duration) -> bool {
        if !self.config.enabled {
            return false;
        }
        match self.last_upload {
            None => false,
            Some(last) => now.saturating_sub(last) >= self.config.watchdog,
        }
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
    fn static_frames_become_idle_and_gate_decode() {
        let mut t = FrameThrottle::new(cfg());
        // First 15 identical frames still decode (below idle threshold).
        for i in 0..15 {
            assert!(t.should_decode(ms(i * 33), b"static"));
        }
        // Now idle: a decode just happened, so an immediate one is gated off...
        assert!(!t.should_decode(ms(15 * 33), b"static"));
        // ...but after idle_decode (100ms) it decodes again.
        assert!(t.should_decode(ms(15 * 33 + 100), b"static"));
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
    fn watchdog_forces_refresh_after_interval() {
        let mut t = FrameThrottle::new(cfg());
        t.should_upload(ms(0), b"pixels");
        assert!(!t.should_force_refresh(ms(500)));
        assert!(t.should_force_refresh(ms(1000))); // watchdog = 1000ms
    }
}
