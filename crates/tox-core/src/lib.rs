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

/// Verify an Ed25519 signature against a Tox public key (32 bytes).
pub fn verify_signature(public_key: &[u8; 32], data: &[u8], signature: &[u8]) -> bool {
    use ed25519_dalek::Verifier;
    let Ok(pk) = ed25519_dalek::VerifyingKey::from_bytes(public_key) else {
        return false;
    };
    let Ok(sig) = ed25519_dalek::Signature::from_slice(signature) else {
        return false;
    };
    pk.verify(data, &sig).is_ok()
}
