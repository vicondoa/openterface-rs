//! USB HID usage tables and keysym→usage mapping.
//!
//! Implemented in **W2.2**. The mapping is split into two paths:
//!
//! - **physical-key forwarding** — window keysym → HID usage (Usage Page 0x07),
//!   modifiers, keypad, function/navigation/media keys, AltGr, and correct
//!   key-release after focus loss;
//! - **text injection** (`sendText`) — a separate path that types a string.
//!
//! W0 leaves this module as a documented placeholder so dependents can name it.

// (Tables and mapping functions land in W2.2.)
