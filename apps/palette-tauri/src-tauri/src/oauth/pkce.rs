//! PKCE (RFC 7636) code-verifier/challenge and CSRF `state` generation.
//! Randomness is sourced from v4 UUIDs (`getrandom` → OS CSPRNG).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// 32 cryptographically-random bytes sourced from two v4 UUIDs (`getrandom`).
fn random_bytes_32() -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    out[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    out
}

/// A fresh PKCE code verifier: base64url(no-pad) of 32 random bytes → 43 chars.
pub(crate) fn generate_code_verifier() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes_32())
}

/// The S256 challenge for a verifier: base64url(no-pad) of SHA-256(verifier).
/// Matches lab-auth's `pkce_challenge` (vendor/lab-auth/src/token.rs:271-273).
pub(crate) fn code_challenge_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A random CSRF `state` value (base64url of 16 random bytes → 22 chars).
pub(crate) fn generate_state() -> String {
    URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes())
}

#[cfg(test)]
#[path = "pkce_tests.rs"]
mod tests;
