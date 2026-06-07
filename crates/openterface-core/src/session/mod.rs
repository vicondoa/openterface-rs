//! Session orchestration: video + input + serial.
//!
//! The session owns the worker threads (capture, paced serial writer, optional
//! GUI loop) and an explicit **shutdown/cancellation model** — a shutdown
//! channel, join policy, bounded queues, and a timeout policy for blocking
//! reads — so the session never deadlocks or hangs on stop, even on device
//! disconnect. Fatal errors are distinguished from recoverable stream errors.
//!
//! Implemented in **W3.2**; W0 leaves it as a documented placeholder.

// (Orchestration + lifecycle lands in W3.2.)
