//! Argon2id password hashing (PHC string format).
//!
//! Uses the `argon2` crate defaults: **Argon2id** v19 with the library's
//! default memory/time/parallelism parameters. Hashes are stored as PHC
//! strings (`$argon2id$v=19$...`) on `accounts.password`.

use argon2::{
  Argon2,
  password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
  #[error("password hash error: {0}")]
  Hash(String),
  #[error("password verify error: {0}")]
  Verify(String),
}

/// Hash a plaintext password with Argon2id (random salt).
///
/// Returns a PHC-encoded string suitable for `accounts.password`.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
  Argon2::default()
    .hash_password(password.as_bytes())
    .map(|h| h.to_string())
    .map_err(|e| PasswordError::Hash(e.to_string()))
}

/// Verify a plaintext password against a PHC hash from `accounts.password`.
///
/// Returns `Ok(true)` on match, `Ok(false)` on mismatch. Parse/algorithm
/// errors surface as [`PasswordError::Verify`].
pub fn verify_password(
  password: &str,
  password_hash: &str,
) -> Result<bool, PasswordError> {
  let parsed = PasswordHash::new(password_hash)
    .map_err(|e| PasswordError::Verify(e.to_string()))?;
  match Argon2::default().verify_password(password.as_bytes(), &parsed) {
    Ok(()) => Ok(true),
    Err(argon2::password_hash::Error::PasswordInvalid) => Ok(false),
    Err(e) => Err(PasswordError::Verify(e.to_string())),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hash_and_verify_round_trip() {
    let hash = hash_password("secret-password").unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert!(verify_password("secret-password", &hash).unwrap());
    assert!(!verify_password("wrong", &hash).unwrap());
  }
}
