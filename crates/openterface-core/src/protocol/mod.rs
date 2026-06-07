//! The CH9329/HID wire protocol.
//!
//! - [`ch9329`] — command framing (`57 AB 00 <CMD> <LEN> <DATA..> <SUM>`).
//! - [`hid`] — USB HID usage tables and keysym→usage mapping.

pub mod ch9329;
pub mod hid;
