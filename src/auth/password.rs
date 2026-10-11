use argon2::{
  Argon2,
  password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use thiserror::Error;

/// Minimum password length, counted in Unicode characters.
pub const MIN_PASSWORD_LEN: usize = 8;
/// Maximum password length, counted in Unicode characters (DoS bound for
/// hashing + strength estimation).
pub const MAX_PASSWORD_LEN: usize = 128;
/// Minimum `zxcvbn` strength score (0-4 scale).
pub const MIN_ZXCVBN_SCORE: u8 = 3;

#[derive(Debug, Error)]
pub enum PasswordError {
  #[error("password hash error: {0}")]
  Hash(String),
  #[error("password verify error: {0}")]
  Verify(String),
  #[error("password must be at least {0} characters")]
  TooShort(usize),
  #[error("password must be at most {0} characters")]
  TooLong(usize),
  #[error("password is too weak: choose a stronger password")]
  TooWeak,
}

/// Validate a candidate password: minimum length, then maximum length, then
/// `zxcvbn` strength.
///
/// Lengths are counted with `chars().count()`, not bytes, so multibyte
/// passwords are not penalized. `user_inputs` should carry the account email,
/// name, and site name (`"lindaflor"`) so passwords built from personal data
/// score lower.
///
/// NOTE (deferred): HaveIBeenPwned k-anonymity breach check is intentionally
/// not done here — it needs an outbound HTTP dependency at signup/reset time.
/// Revisit when the mail/async infra lands; until then `zxcvbn ≥ 3` plus the
/// length bounds are the enforced policy. Never log the candidate password.
pub fn validate_password(
  password: &str,
  user_inputs: &[&str],
) -> Result<(), PasswordError> {
  let len = password.chars().count();
  let min_len = crate::config::password_min_len();
  if len < min_len {
    return Err(PasswordError::TooShort(min_len));
  }
  let max_len = crate::config::password_max_len();
  if len > max_len {
    return Err(PasswordError::TooLong(max_len));
  }
  let min_score = crate::config::password_min_zxcvbn_score();
  let score = u8::from(zxcvbn::zxcvbn(password, user_inputs).score());
  if score < min_score {
    return Err(PasswordError::TooWeak);
  }
  Ok(())
}

/// Hash a plaintext password with Argon2id (random salt).
///
/// Returns a PHC-encoded string suitable for `accounts.password`.
/// Callers must run [`validate_password`] first; this function hashes
/// whatever it is given.
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

  #[test]
  fn rejects_short_password_before_strength_check() {
    let err = validate_password("Ab1!", &[]).unwrap_err();
    assert!(
      err.to_string().contains("at least"),
      "unexpected error: {err}"
    );
    assert!(matches!(err, PasswordError::TooShort(_)));
  }

  #[test]
  fn rejects_password_over_max_chars() {
    let long = "a".repeat(MAX_PASSWORD_LEN + 1);
    assert_eq!(long.chars().count(), MAX_PASSWORD_LEN + 1);
    let err = validate_password(&long, &[]).unwrap_err();
    assert!(err.to_string().contains("at most"), "unexpected: {err}");
    assert!(matches!(err, PasswordError::TooLong(_)));
  }

  #[test]
  fn max_counts_chars_not_bytes_for_multibyte() {
    // 129 `é` = 258 bytes but 129 chars → TooLong (not a byte count).
    let long = "é".repeat(129);
    assert_eq!(long.chars().count(), 129);
    assert!(long.len() > MAX_PASSWORD_LEN);
    let err = validate_password(&long, &[]).unwrap_err();
    assert!(err.to_string().contains("at most"), "unexpected: {err}");
    // 100 `é` = 200 bytes but only 100 chars → passes the length
    // bound and falls through to the strength check instead.
    let fits = "é".repeat(100);
    assert_eq!(fits.chars().count(), 100);
    let err = validate_password(&fits, &[]).unwrap_err();
    assert!(
      err.to_string().contains("too weak"),
      "multibyte length must count chars, got: {err}"
    );
  }

  #[test]
  fn rejects_weak_password() {
    let err =
      validate_password("password123", &["user@example.com"]).unwrap_err();
    assert!(err.to_string().contains("too weak"), "unexpected: {err}");
    assert!(matches!(err, PasswordError::TooWeak));
  }

  #[test]
  fn min_is_checked_before_max_and_strength() {
    // Short input reports TooShort even though it is also weak.
    let err = validate_password("pw", &[]).unwrap_err();
    assert!(matches!(err, PasswordError::TooShort(_)));
  }

  #[test]
  fn accepts_strong_password_with_user_inputs() {
    for candidate in [
      "Harbor-Violet-Tiger-4821-Lantern!",
      "Quilt-Mango-Brave-Pixel-7395?",
    ] {
      assert!(
        validate_password(
          candidate,
          &["user@example.com", "Test User", "lindaflor"],
        )
        .is_ok(),
        "strong password rejected: {candidate}"
      );
    }
  }
}
