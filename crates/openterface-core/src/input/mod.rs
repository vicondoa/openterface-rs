//! Input-event mapping and forwarding glue.
//!
//! This module wires window-sourced [`crate::event::InputEvent`]s to CH9329
//! command builders, including absolute vs relative mouse modes (long-press
//! Esc exits relative) and the special-key combinations. The forwarding path
//! feeds the [`crate::pacing`] scheduler rather than the transport directly.
//!
//! Implemented in **W3.2**; W0 leaves it as a documented placeholder.

// (Mapping + forwarding glue lands in W3.2.)
