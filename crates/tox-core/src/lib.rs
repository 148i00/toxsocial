//! Safe Rust wrapper around c-toxcore.
//!
//! [`ToxSession`] owns a single `Tox*` instance, runs the mandatory
//! `tox_iterate` loop on a background thread, and converts C callbacks into
//! typed [`Event`]s delivered over a channel. No C pointer ever escapes this
//! crate.

pub mod bootstrap;
pub mod error;
pub mod event;
pub mod session;

pub use bootstrap::DEFAULT_BOOTSTRAP_NODES;
pub use error::ToxError;
pub use event::{Connection, Event, Status};
pub use session::{ToxSession, MAX_NAME_LENGTH, MAX_STATUS_MESSAGE_LENGTH};

/// Re-export of the raw FFI for advanced uses (e.g. writing tests).
pub mod ffi {
    pub use tox_ffi::*;
}
