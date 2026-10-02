//! TOTP + backup-code helpers for two-factor auth.

use rand::Rng;
use totp_rs::{Algorithm, Builder, Secret};

pub const ISSUER: &str = "lindaflor";
const BACKUP_CODE_COUNT: usize = 10;

/// Generate a new TOTP secret (base32) and otpauth URI for `account` (usually email).
pub fn generate_secret(account: &str) -> Result<(String, String), String> {
  let secret = Secret::generate();
  let secret_b32 = secret.to_base32();
  let totp = Builder::new()
    .with_algorithm(Algorithm::SHA1)
    .with_digits(6)
    .with_skew(1)
    .with_step_duration(30)
    .with_secret(secret)
    .with_issuer(Some(ISSUER))
    .with_account_name(account)
    .build()
    .map_err(|e| e.to_string())?;
  let uri = totp.to_url().map_err(|e| e.to_string())?;
  Ok((secret_b32, uri))
}

/// Verify a 6-digit TOTP code against a stored base32 secret.
pub fn verify_code(secret_b32: &str, code: &str) -> Result<bool, String> {
  let code = code.trim();
  if code.len() != 6 || !code.chars().all(|c| c.is_ascii_digit()) {
    return Ok(false);
  }
  let secret =
    Secret::try_from_base32(secret_b32).map_err(|e| e.to_string())?;
  let totp = Builder::new()
    .with_algorithm(Algorithm::SHA1)
    .with_digits(6)
    .with_skew(1)
    .with_step_duration(30)
    .with_secret(secret)
    .build()
    .map_err(|e| e.to_string())?;
  Ok(totp.check_current(code).is_some())
}

/// Generate printable backup codes (`XXXX-XXXX` style).
pub fn generate_backup_codes() -> Vec<String> {
  let mut rng = rand::thread_rng();
  (0..BACKUP_CODE_COUNT)
    .map(|_| {
      let a: u16 = rng.gen_range(0..10_000);
      let b: u16 = rng.gen_range(0..10_000);
      format!("{a:04}-{b:04}")
    })
    .collect()
}

/// Encode backup codes for storage.
///
/// TODO: hash or encrypt before production; plaintext comma-separated for now.
pub fn encode_backup_codes(codes: &[String]) -> String {
  codes.join(",")
}

/// Decode stored backup codes.
pub fn decode_backup_codes(stored: &str) -> Vec<String> {
  stored
    .split(',')
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned)
    .collect()
}

/// Constant-time-ish consume: remove one matching backup code (case-insensitive, dashes optional).
/// Returns the remaining codes when a match was found.
pub fn consume_backup_code(
  stored: &str,
  presented: &str,
) -> Option<Vec<String>> {
  let needle = normalize_backup_code(presented);
  if needle.is_empty() {
    return None;
  }
  let mut codes = decode_backup_codes(stored);
  let idx = codes
    .iter()
    .position(|c| normalize_backup_code(c) == needle)?;
  codes.remove(idx);
  Some(codes)
}

fn normalize_backup_code(code: &str) -> String {
  code
    .chars()
    .filter(|c| c.is_ascii_alphanumeric())
    .map(|c| c.to_ascii_uppercase())
    .collect()
}
