//! `openterface-core` — the device-agnostic core of openterface-rs.
//!
//! This crate contains no GUI, no async runtime, and no hard dependency on a
//! physical device. Everything that touches hardware is expressed as a trait
//! ([`serial::SerialTransport`], [`video::VideoSource`],
//! [`discovery::DeviceScanner`]) so the full pipeline can be exercised against
//! simulated devices in [`openterface-test-support`]. This is what lets the
//! test-suite run with **zero hardware**.
//!
//! ## Module map
//!
//! - [`protocol`] — CH9329 wire framing and HID usage tables.
//! - [`serial`] — byte-level serial transport contract + baud constants.
//! - [`video`] — capture contract and the [`video::Frame`] model.
//! - [`decode`] — MJPEG / YUYV → RGBA decoding.
//! - [`discovery`] — Openterface device enumeration contract.
//! - [`input`] — input-event mapping and forwarding glue.
//! - [`pacing`] — the paced CH9329 command scheduler.
//! - [`session`] — orchestration of video + input + serial.
//! - [`device`] — USB identity constants for the Openterface endpoints.
//! - [`event`] — the device-agnostic input-event model.

pub mod decode;
pub mod device;
pub mod discovery;
pub mod error;
pub mod event;
pub mod input;
pub mod pacing;
pub mod protocol;
pub mod serial;
pub mod session;
pub mod video;

pub use error::{Error, Result};
