//! The paced CH9329 command scheduler.
//!
//! **Why this exists (load-bearing parity):** over USB/IP the CH9329 drains
//! absolute-move commands at only ~30–40/sec. Forwarding mouse moves faster
//! (e.g. at 60 Hz) overruns its command buffer; the sparse key/button
//! **release** commands then queue behind the move backlog and arrive late, so
//! the target autorepeats keys and clicks miss. The scheduler therefore paces
//! mouse moves (default ~30 Hz), coalesces moves, and lets releases jump ahead
//! of the move backlog.
//!
//! W3.1 implements the queue/backpressure/coalescing scheduler with a fake
//! clock for deterministic tests. W0 defines the configuration surface (with
//! the env-var contract) so dependents compile against the final API.

use std::time::Duration;

/// Default mouse-move forward interval (~30 Hz). Tunable via
/// `OPENTERFACE_MOUSE_INTERVAL_MS`.
pub const DEFAULT_MOUSE_INTERVAL: Duration = Duration::from_millis(33);

/// Environment variable that overrides the mouse-move interval (milliseconds).
pub const ENV_MOUSE_INTERVAL_MS: &str = "OPENTERFACE_MOUSE_INTERVAL_MS";

/// Configuration for the pacing scheduler.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PacingConfig {
    /// Minimum interval between forwarded mouse-move commands.
    pub mouse_interval: Duration,
}

impl Default for PacingConfig {
    fn default() -> Self {
        Self {
            mouse_interval: DEFAULT_MOUSE_INTERVAL,
        }
    }
}

impl PacingConfig {
    /// Builds a config, applying `OPENTERFACE_MOUSE_INTERVAL_MS` if set and
    /// parseable; otherwise the [`DEFAULT_MOUSE_INTERVAL`].
    #[must_use]
    pub fn from_env() -> Self {
        let mouse_interval = std::env::var(ENV_MOUSE_INTERVAL_MS)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_MOUSE_INTERVAL);
        Self { mouse_interval }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_about_30hz() {
        assert_eq!(
            PacingConfig::default().mouse_interval,
            Duration::from_millis(33)
        );
    }

    #[test]
    fn env_override_parses() {
        // Use a unique scope; std::env is process-global, so set+remove here.
        std::env::set_var(ENV_MOUSE_INTERVAL_MS, "50");
        assert_eq!(
            PacingConfig::from_env().mouse_interval,
            Duration::from_millis(50)
        );
        std::env::remove_var(ENV_MOUSE_INTERVAL_MS);
        assert_eq!(
            PacingConfig::from_env().mouse_interval,
            DEFAULT_MOUSE_INTERVAL
        );
    }
}
