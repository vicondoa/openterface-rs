//! The paced CH9329 command scheduler.
//!
//! **Why this exists (load-bearing parity):** over USB/IP the CH9329 drains
//! absolute-move commands at only ~30-40/sec. Forwarding mouse moves faster
//! (e.g. at 60 Hz) overruns its command buffer; the sparse key/button
//! **release** commands then queue behind the move backlog and arrive late, so
//! the target autorepeats keys and clicks miss. The scheduler therefore:
//!
//! - **paces mouse moves** to one per [`PacingConfig::mouse_interval`] (default
//!   ~30 Hz), and
//! - **coalesces** consecutive moves (only the latest position matters), while
//! - **key/button/scroll commands jump ahead of the move backlog** (a priority
//!   queue) so releases are never delayed.
//!
//! [`PacingScheduler::poll`] takes the current time explicitly, so the whole
//! state machine is deterministic and testable with no real clock. The W3.2
//! session writer thread drives it with `Instant::now()`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::event::{AbsPosition, ButtonMask, HidUsage, InputEvent, Modifiers};
use crate::protocol::ch9329;

/// Default mouse-move forward interval (~30 Hz). Tunable via
/// `OPENTERFACE_MOUSE_INTERVAL_MS`.
pub const DEFAULT_MOUSE_INTERVAL: Duration = Duration::from_millis(33);

/// Environment variable that overrides the mouse-move interval (milliseconds).
pub const ENV_MOUSE_INTERVAL_MS: &str = "OPENTERFACE_MOUSE_INTERVAL_MS";

/// Minimum / maximum accepted mouse interval (matches the C++ 5..=1000 ms gate).
const MIN_INTERVAL_MS: u64 = 5;
const MAX_INTERVAL_MS: u64 = 1000;

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
    /// Builds a config, applying `OPENTERFACE_MOUSE_INTERVAL_MS` if set and in
    /// the accepted `5..=1000` ms range; otherwise the [`DEFAULT_MOUSE_INTERVAL`].
    #[must_use]
    pub fn from_env() -> Self {
        let mouse_interval = std::env::var(ENV_MOUSE_INTERVAL_MS)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|ms| (MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(ms))
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_MOUSE_INTERVAL);
        Self { mouse_interval }
    }
}

/// The coalesced pending mouse motion, if any.
#[derive(Clone, Copy, Debug)]
enum PendingMove {
    /// Latest absolute position (replaces any prior pending absolute move).
    Absolute(AbsPosition),
    /// Accumulated relative delta (saturating).
    Relative { dx: i32, dy: i32 },
}

/// A paced CH9329 command scheduler. Pure logic: feed it [`InputEvent`]s with
/// [`PacingScheduler::submit`] and pull ready CH9329 frames with
/// [`PacingScheduler::poll`].
pub struct PacingScheduler {
    config: PacingConfig,
    /// Commands that must be sent promptly and in order (keys, buttons, scroll,
    /// including all **releases**). These bypass move pacing.
    priority: VecDeque<Vec<u8>>,
    /// The coalesced pending mouse move (paced).
    pending_move: Option<PendingMove>,
    /// When the last paced move was emitted (`None` until the first move).
    last_move_at: Option<Instant>,
    /// Current pressed-button mask (carried on mouse commands).
    buttons: ButtonMask,
    /// Last known absolute position (so button events can re-send position).
    last_abs: Option<AbsPosition>,
    /// Whether the most recent motion was relative. Button/scroll frames follow
    /// the active mode so a stale absolute coordinate is never re-asserted after
    /// the session has switched to relative motion.
    relative_mode: bool,
    /// Non-modifier HID usages currently held (the 6-key report array).
    held_keys: Vec<HidUsage>,
    /// Current modifier byte (from the latest key event).
    modifiers: Modifiers,
}

impl PacingScheduler {
    /// Creates a scheduler with the given configuration.
    #[must_use]
    pub fn new(config: PacingConfig) -> Self {
        Self {
            config,
            priority: VecDeque::new(),
            pending_move: None,
            last_move_at: None,
            buttons: ButtonMask::NONE,
            last_abs: None,
            relative_mode: false,
            held_keys: Vec::new(),
            modifiers: Modifiers::NONE,
        }
    }

    /// Submits an input event for forwarding.
    pub fn submit(&mut self, event: InputEvent) {
        match event {
            InputEvent::MouseMoveAbsolute { pos } => {
                self.relative_mode = false;
                self.last_abs = Some(pos);
                self.pending_move = Some(PendingMove::Absolute(pos));
            }
            InputEvent::MouseMoveRelative { dx, dy } => {
                self.relative_mode = true;
                let acc = match self.pending_move {
                    Some(PendingMove::Relative { dx: ax, dy: ay }) => PendingMove::Relative {
                        dx: ax + i32::from(dx),
                        dy: ay + i32::from(dy),
                    },
                    _ => PendingMove::Relative {
                        dx: i32::from(dx),
                        dy: i32::from(dy),
                    },
                };
                self.pending_move = Some(acc);
            }
            InputEvent::MouseButton { button, pressed } => {
                // Flush any pending relative move with the *current* (pre-button)
                // mask first, so the movement frame carries the old button state,
                // then apply the button change and enqueue the button command.
                self.flush_pending_relative_move();
                self.buttons = self.buttons.with(button, pressed);
                self.priority.push_back(self.mouse_button_command());
            }
            InputEvent::Key {
                usage,
                modifiers,
                pressed,
            } => {
                self.modifiers = modifiers;
                self.update_held(usage, pressed);
                self.priority
                    .push_back(ch9329::keyboard(self.modifiers, &self.held_keys));
            }
            InputEvent::Scroll { delta } => {
                // Scroll is a relative CH9329 frame carrying the wheel byte.
                self.flush_pending_relative_move();
                self.priority
                    .push_back(ch9329::mouse_relative(0, 0, self.buttons, delta));
            }
        }
    }

    /// Returns the next CH9329 frame ready to send at `now`, or `None` if
    /// nothing is ready yet. Priority (key/button/scroll) commands are returned
    /// immediately; a coalesced mouse move is returned only once per
    /// `mouse_interval`.
    pub fn poll(&mut self, now: Instant) -> Option<Vec<u8>> {
        if let Some(cmd) = self.priority.pop_front() {
            return Some(cmd);
        }
        if self.pending_move.is_some() && self.move_due(now) {
            let mv = self.pending_move.take().unwrap();
            self.last_move_at = Some(now);
            return Some(self.move_command(mv));
        }
        None
    }

    /// Returns how long until the scheduler next has something to send at `now`,
    /// or `None` if it is idle. `Some(Duration::ZERO)` means "ready now".
    #[must_use]
    pub fn time_until_ready(&self, now: Instant) -> Option<Duration> {
        if !self.priority.is_empty() {
            return Some(Duration::ZERO);
        }
        let pending = self.pending_move?;
        let _ = pending;
        match self.last_move_at {
            None => Some(Duration::ZERO),
            Some(last) => {
                let elapsed = now.saturating_duration_since(last);
                Some(self.config.mouse_interval.saturating_sub(elapsed))
            }
        }
    }

    fn move_due(&self, now: Instant) -> bool {
        match self.last_move_at {
            None => true,
            Some(last) => now.saturating_duration_since(last) >= self.config.mouse_interval,
        }
    }

    fn move_command(&self, mv: PendingMove) -> Vec<u8> {
        match mv {
            PendingMove::Absolute(pos) => ch9329::mouse_absolute(pos, self.buttons, 0),
            PendingMove::Relative { dx, dy } => {
                let dx = dx.clamp(-127, 127) as i8;
                let dy = dy.clamp(-127, 127) as i8;
                ch9329::mouse_relative(dx, dy, self.buttons, 0)
            }
        }
    }

    fn flush_pending_relative_move(&mut self) {
        // In relative mode a position-dependent priority command (button/scroll)
        // must not jump ahead of an un-sent relative move, or it would apply at
        // the pre-move position. A zero-delta relative button frame cannot carry
        // the pending delta, so flush the accumulated move ahead of the command.
        if self.relative_mode {
            if let Some(mv @ PendingMove::Relative { .. }) = self.pending_move {
                let cmd = self.move_command(mv);
                self.priority.push_back(cmd);
                self.pending_move = None;
            }
        }
    }

    fn mouse_button_command(&self) -> Vec<u8> {
        // Follow the active motion mode so we never re-assert a stale absolute
        // coordinate after switching to relative motion.
        if self.relative_mode {
            return ch9329::mouse_relative(0, 0, self.buttons, 0);
        }
        match self.last_abs {
            Some(pos) => ch9329::mouse_absolute(pos, self.buttons, 0),
            None => ch9329::mouse_relative(0, 0, self.buttons, 0),
        }
    }

    fn update_held(&mut self, usage: HidUsage, pressed: bool) {
        // Modifier keys live in the modifier byte, not the 6-key array.
        if crate::protocol::hid::is_modifier(usage) {
            return;
        }
        if pressed {
            if !self.held_keys.contains(&usage) && self.held_keys.len() < ch9329::MAX_KEYS {
                self.held_keys.push(usage);
            }
        } else {
            self.held_keys.retain(|&k| k != usage);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseButton;

    fn sched() -> PacingScheduler {
        PacingScheduler::new(PacingConfig::default())
    }

    #[test]
    fn default_is_about_30hz() {
        assert_eq!(
            PacingConfig::default().mouse_interval,
            Duration::from_millis(33)
        );
    }

    #[test]
    fn env_override_in_range() {
        std::env::set_var(ENV_MOUSE_INTERVAL_MS, "50");
        assert_eq!(
            PacingConfig::from_env().mouse_interval,
            Duration::from_millis(50)
        );
        // Out-of-range values are ignored.
        std::env::set_var(ENV_MOUSE_INTERVAL_MS, "2");
        assert_eq!(
            PacingConfig::from_env().mouse_interval,
            DEFAULT_MOUSE_INTERVAL
        );
        std::env::set_var(ENV_MOUSE_INTERVAL_MS, "99999");
        assert_eq!(
            PacingConfig::from_env().mouse_interval,
            DEFAULT_MOUSE_INTERVAL
        );
        std::env::remove_var(ENV_MOUSE_INTERVAL_MS);
    }

    #[test]
    fn first_move_is_immediate_then_paced() {
        let mut s = sched();
        let t0 = Instant::now();
        s.submit(InputEvent::MouseMoveAbsolute {
            pos: AbsPosition { x: 100, y: 100 },
        });
        // First move goes out immediately.
        assert!(s.poll(t0).is_some());
        // A second move right away is held until the interval elapses.
        s.submit(InputEvent::MouseMoveAbsolute {
            pos: AbsPosition { x: 200, y: 200 },
        });
        assert!(s.poll(t0).is_none());
        assert!(s.poll(t0 + Duration::from_millis(10)).is_none());
        assert!(s.poll(t0 + Duration::from_millis(33)).is_some());
    }

    #[test]
    fn moves_coalesce_to_latest_position() {
        let mut s = sched();
        let t0 = Instant::now();
        // Prime: emit one move so the next is paced.
        s.submit(InputEvent::MouseMoveAbsolute {
            pos: AbsPosition { x: 1, y: 1 },
        });
        let _ = s.poll(t0);
        // Several moves during the interval coalesce to the last.
        for x in [10u16, 20, 30, 40] {
            s.submit(InputEvent::MouseMoveAbsolute {
                pos: AbsPosition { x, y: x },
            });
        }
        let cmd = s.poll(t0 + Duration::from_millis(33)).unwrap();
        // Absolute frame: bytes 7..9 are xLo,xHi. 40 = 0x28.
        assert_eq!(cmd[3], ch9329::cmd::MOUSE_ABS);
        assert_eq!(cmd[7], 40);
        // Only one coalesced move was queued.
        assert!(s.poll(t0 + Duration::from_millis(100)).is_none());
    }

    #[test]
    fn release_jumps_ahead_of_move_backlog() {
        let mut s = sched();
        let t0 = Instant::now();
        // Emit the first move so subsequent moves are paced (backlogged).
        s.submit(InputEvent::MouseMoveAbsolute {
            pos: AbsPosition { x: 1, y: 1 },
        });
        let _ = s.poll(t0);
        // A move is now pending and paced (not yet due)...
        s.submit(InputEvent::MouseMoveAbsolute {
            pos: AbsPosition { x: 9, y: 9 },
        });
        // ...but a key release arrives. It must be sent immediately, ahead of
        // the still-waiting move.
        s.submit(InputEvent::Key {
            usage: HidUsage(0x04),
            modifiers: Modifiers::NONE,
            pressed: false,
        });
        let cmd = s.poll(t0).expect("release should be ready now");
        assert_eq!(cmd[3], ch9329::cmd::KEYBOARD);
        // The move is still pending until its interval elapses.
        assert!(s.poll(t0).is_none());
    }

    #[test]
    fn key_press_and_release_build_reports() {
        let mut s = sched();
        let t0 = Instant::now();
        s.submit(InputEvent::Key {
            usage: HidUsage(0x04), // 'a'
            modifiers: Modifiers::LEFT_SHIFT,
            pressed: true,
        });
        let press = s.poll(t0).unwrap();
        assert_eq!(press[3], ch9329::cmd::KEYBOARD);
        assert_eq!(press[5], Modifiers::LEFT_SHIFT.0); // modifier byte
        assert_eq!(press[7], 0x04); // first key
        s.submit(InputEvent::Key {
            usage: HidUsage(0x04),
            modifiers: Modifiers::NONE,
            pressed: false,
        });
        let release = s.poll(t0).unwrap();
        // All-zero key array after release.
        assert_eq!(&release[7..13], &[0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn modifier_keys_do_not_fill_key_array() {
        let mut s = sched();
        let t0 = Instant::now();
        s.submit(InputEvent::Key {
            usage: HidUsage(0xE1), // Left Shift as a key event
            modifiers: Modifiers::LEFT_SHIFT,
            pressed: true,
        });
        let cmd = s.poll(t0).unwrap();
        // Modifier byte set, but the 6-key array stays empty.
        assert_eq!(cmd[5], Modifiers::LEFT_SHIFT.0);
        assert_eq!(&cmd[7..13], &[0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn button_command_carries_mask_at_last_position() {
        let mut s = sched();
        let t0 = Instant::now();
        s.submit(InputEvent::MouseMoveAbsolute {
            pos: AbsPosition { x: 500, y: 600 },
        });
        let _ = s.poll(t0);
        s.submit(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        });
        let cmd = s.poll(t0).unwrap();
        assert_eq!(cmd[3], ch9329::cmd::MOUSE_ABS);
        assert_eq!(cmd[6], ButtonMask::LEFT.0); // button mask byte
                                                // x=500=0x01F4 -> lo 0xF4 hi 0x01.
        assert_eq!(cmd[7], 0xF4);
        assert_eq!(cmd[8], 0x01);
    }

    #[test]
    fn scroll_uses_relative_wheel_byte() {
        let mut s = sched();
        let t0 = Instant::now();
        s.submit(InputEvent::Scroll { delta: 1 });
        let cmd = s.poll(t0).unwrap();
        assert_eq!(cmd[3], ch9329::cmd::MOUSE_REL);
        assert_eq!(cmd[9], 0x01); // wheel up
    }

    #[test]
    fn relative_moves_accumulate_and_clamp() {
        let mut s = sched();
        let t0 = Instant::now();
        // Prime.
        s.submit(InputEvent::MouseMoveRelative { dx: 1, dy: 1 });
        let _ = s.poll(t0);
        // Accumulate beyond i8 range.
        for _ in 0..50 {
            s.submit(InputEvent::MouseMoveRelative { dx: 10, dy: -10 });
        }
        let cmd = s.poll(t0 + Duration::from_millis(33)).unwrap();
        assert_eq!(cmd[3], ch9329::cmd::MOUSE_REL);
        assert_eq!(cmd[7] as i8, 127); // dx clamped
        assert_eq!(cmd[8] as i8, -127); // dy clamped
    }

    #[test]
    fn button_uses_relative_frame_after_relative_motion() {
        let mut s = sched();
        let t0 = Instant::now();
        // Establish an absolute position first...
        s.submit(InputEvent::MouseMoveAbsolute {
            pos: AbsPosition { x: 500, y: 600 },
        });
        let _ = s.poll(t0);
        // ...then switch to relative motion.
        s.submit(InputEvent::MouseMoveRelative { dx: 3, dy: 3 });
        let _ = s.poll(t0 + Duration::from_millis(33));
        // A button now must be a RELATIVE frame, not an absolute one at the
        // stale (500,600) coordinate.
        s.submit(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        });
        let cmd = s.poll(t0 + Duration::from_millis(33)).unwrap();
        assert_eq!(cmd[3], ch9329::cmd::MOUSE_REL);
        assert_eq!(cmd[6], ButtonMask::LEFT.0);
    }

    #[test]
    fn relative_move_flushes_before_click_in_same_batch() {
        let mut s = sched();
        let t0 = Instant::now();
        // Enter relative mode and accumulate a move, all before any poll.
        s.submit(InputEvent::MouseMoveRelative { dx: 7, dy: -4 });
        s.submit(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        });
        // The move must come out FIRST (with its delta and the pre-click button
        // state), then the click.
        let first = s.poll(t0).unwrap();
        assert_eq!(first[3], ch9329::cmd::MOUSE_REL);
        assert_eq!(first[6], ButtonMask::NONE.0); // move carries no button yet
        assert_eq!(first[7] as i8, 7); // dx
        assert_eq!(first[8] as i8, -4); // dy
        let second = s.poll(t0).unwrap();
        assert_eq!(second[3], ch9329::cmd::MOUSE_REL);
        assert_eq!(second[6], ButtonMask::LEFT.0); // the click
                                                   // No leftover pending move.
        assert!(s.poll(t0 + Duration::from_millis(100)).is_none());
    }

    #[test]
    fn drag_release_keeps_button_until_after_final_move() {
        let mut s = sched();
        let t0 = Instant::now();
        // Press (button down) in relative mode, then a drag move, then release.
        s.submit(InputEvent::MouseMoveRelative { dx: 0, dy: 0 });
        let _ = s.poll(t0); // emit the priming move
        s.submit(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
        });
        let _ = s.poll(t0); // emit the button-down
        s.submit(InputEvent::MouseMoveRelative { dx: 5, dy: 5 }); // drag
        s.submit(InputEvent::MouseButton {
            button: MouseButton::Left,
            pressed: false,
        });
        // The drag move must be flushed with LEFT still held, before the release.
        let drag = s.poll(t0).unwrap();
        assert_eq!(drag[3], ch9329::cmd::MOUSE_REL);
        assert_eq!(
            drag[6],
            ButtonMask::LEFT.0,
            "drag segment keeps button held"
        );
        assert_eq!(drag[7] as i8, 5);
        let release = s.poll(t0).unwrap();
        assert_eq!(release[6], ButtonMask::NONE.0, "release clears the button");
    }

    #[test]
    fn idle_scheduler_reports_no_readiness() {
        let s = sched();
        assert_eq!(s.time_until_ready(Instant::now()), None);
    }
}
