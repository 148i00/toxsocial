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
///
/// A Tox public key is an X25519 key. ToxSocial signs public posts by
/// interpreting the same 32-byte secret seed as an Ed25519 seed, so the
/// signing key's Edwards public key is the *birational image* of the X25519
/// public key: `y = (u - 1) / (u + 1) mod p` (RFC 8032 / RFC 7748). The X25519
/// key does not carry the Edwards x sign bit, so we try both sign bits.
pub fn verify_signature(public_key: &[u8; 32], data: &[u8], signature: &[u8]) -> bool {
    use num_bigint::BigUint;
    use num_traits::One;
    let Ok(sig) = ed25519_dalek::Signature::from_slice(signature) else {
        return false;
    };
    // X25519 public keys only use the low 255 bits; p = 2^255 - 19.
    let mut u_bytes = *public_key;
    u_bytes[31] &= 0x7f;
    let p = (BigUint::one() << 255) - 19u8;
    let u = BigUint::from_bytes_le(&u_bytes);
    // y = (u - 1) / (u + 1) mod p = (u - 1) * (u + 1)^(p - 2) mod p.
    let one = BigUint::one();
    let two = BigUint::from(2u8);
    let num: BigUint = (&u + &one) % &p;
    let y = ((&u + &p - &one) % &p) * num.modpow(&(&p - &two), &p) % &p;
    let mut y_bytes = [0u8; 32];
    let raw = y.to_bytes_le();
    y_bytes[..raw.len()].copy_from_slice(&raw);
    for sign in [0u8, 1u8] {
        y_bytes[31] = (y_bytes[31] & 0x7f) | (sign << 7);
        let Ok(pk) = ed25519_dalek::VerifyingKey::from_bytes(&y_bytes) else {
            continue;
        };
        use ed25519_dalek::Verifier;
        if pk.verify(data, &sig).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;
    use num_traits::One;

    /// Edwards y -> X25519 u via the birational map: u = (1 + y) / (1 - y).
    fn edwards_y_to_x25519_u(y_bytes: &[u8; 32]) -> [u8; 32] {
        let p = (BigUint::one() << 255) - 19u8;
        let mut y_in = *y_bytes;
        y_in[31] &= 0x7f; // drop the Edwards sign bit
        let y = BigUint::from_bytes_le(&y_in);
        let one = BigUint::one();
        let two = BigUint::from(2u8);
        let num: BigUint = (&one + &p - &y) % &p;
        let u = ((&one + &y) % &p) * num.modpow(&(&p - &two), &p) % &p;
        let mut b = [0u8; 32];
        let raw = u.to_bytes_le();
        b[..raw.len()].copy_from_slice(&raw);
        b[31] &= 0x7f; // X25519 ignores the top bit
        b
    }

    #[test]
    fn verify_signature_accepts_x25519_public_key() {
        use ed25519_dalek::Signer;
        // Same seed interpreted as Ed25519 seed (as ToxSocial does with the
        // Tox secret key) -> real Edwards pk -> its X25519 image.
        let seed = [42u8; 32];
        let signing = ed25519_dalek::SigningKey::from_bytes(&seed);
        let data = b"id|author|123|hello|true";
        let sig = signing.sign(data).to_bytes();
        let x_pk = edwards_y_to_x25519_u(&signing.verifying_key().to_bytes());
        assert!(
            verify_signature(&x_pk, data, &sig),
            "verification must succeed through the birational map"
        );
        // Wrong message must fail.
        assert!(!verify_signature(&x_pk, b"tampered", &sig));
    }

    #[test]
    fn verify_signature_rejects_garbage() {
        let pk = [1u8; 32];
        assert!(!verify_signature(&pk, b"x", &[0u8; 64]));
    }
}
