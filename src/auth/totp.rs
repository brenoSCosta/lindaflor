use rand::{Rng, RngCore};
use sha2::{Digest, Sha256};
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

/// Hash backup codes for storage: `hex(salt)$hex(SHA-256(salt || normalized))`, comma-joined.
pub fn hash_backup_codes(codes: &[String]) -> String {
  let mut rng = rand::thread_rng();
  codes
    .iter()
    .map(|code| {
      let normalized = normalize_backup_code(code);
      let mut salt = [0u8; 16];
      rng.fill_bytes(&mut salt);
      let mut hasher = Sha256::new();
      hasher.update(salt);
      hasher.update(normalized.as_bytes());
      let hash = hasher.finalize();
      format!("{}${}", hex::encode(salt), hex::encode(hash))
    })
    .collect::<Vec<_>>()
    .join(",")
}

/// Consume one matching backup code. Returns remaining stored entries on match.
pub fn consume_backup_code(stored: &str, presented: &str) -> Option<String> {
  let needle = normalize_backup_code(presented);
  if needle.is_empty() {
    return None;
  }
  let mut entries: Vec<(Vec<u8>, Vec<u8>, String)> = Vec::new();
  for part in stored.split(',') {
    let part = part.trim();
    if part.is_empty() {
      continue;
    }
    let (salt_hex, hash_hex) = match part.split_once('$') {
      Some(pair) => pair,
      None => continue,
    };
    let salt = match hex::decode(salt_hex.trim()) {
      Ok(s) => s,
      Err(_) => continue,
    };
    let expected = match hex::decode(hash_hex.trim()) {
      Ok(h) => h,
      Err(_) => continue,
    };
    if salt.len() != 16 || expected.len() != 32 {
      continue;
    }
    entries.push((salt, expected, part.to_owned()));
  }
  let mut match_idx: Option<usize> = None;
  for (idx, (salt, expected, _)) in entries.iter().enumerate() {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(needle.as_bytes());
    let candidate = hasher.finalize();
    if candidate.len() != expected.len() {
      continue;
    }
    let mut diff = 0u8;
    for (a, b) in candidate.iter().zip(expected.iter()) {
      diff |= a ^ b;
    }
    if diff == 0 {
      match_idx = Some(idx);
      break;
    }
  }
  let idx = match_idx?;
  entries.remove(idx);
  Some(
    entries
      .into_iter()
      .map(|(_, _, raw)| raw)
      .collect::<Vec<_>>()
      .join(","),
  )
}
fn normalize_backup_code(code: &str) -> String {
  code
    .chars()
    .filter(|c| c.is_ascii_alphanumeric())
    .map(|c| c.to_ascii_uppercase())
    .collect()
}

/// Valkey/in-memory single-use key claiming one TOTP code for one user
/// (`skew=1` without a replay cache accepts the
/// same code twice inside the window).
///
/// The key embeds only the *normalized* code plus the user id — never the
/// TOTP secret. Enforcement lives in `crate::valkey::totp_reserve`, which
/// claims the key with `SET NX EX 90s` (90s covers the prev/current/next
/// 30s steps). Callers must reserve *after* `verify_code` returns true
/// (and after the pending-token binding check) so a wrong code does not
/// occupy a usable slot; a code that verifies once cannot verify again.
pub fn totp_reuse_key(user_id: &uuid::Uuid, code: &str) -> String {
  format!("totp:used:{}:{}", user_id.as_simple(), code.trim())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn round_trip_consume_each_once() {
    let codes = generate_backup_codes();
    assert_eq!(codes.len(), 10);
    let mut stored = hash_backup_codes(&codes);
    for code in &codes {
      let remaining = consume_backup_code(&stored, code);
      assert!(remaining.is_some(), "code {code} should match");
      stored = remaining.unwrap();
      // Reuse must fail.
      assert!(
        consume_backup_code(&stored, code).is_none(),
        "code {code} reuse should fail"
      );
    }
    assert!(stored.is_empty(), "all entries consumed, got: {stored}");
  }

  #[test]
  fn normalization_variants_match() {
    let codes = vec!["ABCD-1234".to_string()];
    let stored = hash_backup_codes(&codes);
    // lowercase + no dash
    assert!(consume_backup_code(&stored, "abcd1234").is_some());
    assert!(consume_backup_code(&stored, "abcd-1234").is_some());
    assert!(consume_backup_code(&stored, "ABCD1234").is_some());
    assert!(consume_backup_code(&stored, "  abcd-1234  ").is_some());
  }

  #[test]
  fn wrong_code_returns_none() {
    let codes = vec!["ABCD-1234".to_string()];
    let stored = hash_backup_codes(&codes);
    assert!(consume_backup_code(&stored, "ZZZZ-9999").is_none());
    assert!(consume_backup_code(&stored, "").is_none());
    assert!(consume_backup_code(&stored, "---").is_none());
    // Stored must be unchanged (still consumable with right code).
    assert!(consume_backup_code(&stored, "ABCD-1234").is_some());
  }

  #[test]
  fn tampered_stored_hash_does_not_match() {
    let codes = vec!["ABCD-1234".to_string()];
    let stored = hash_backup_codes(&codes);
    let (salt_hex, hash_hex) =
      stored.split_once('$').expect("stored entry has salt$hash");
    assert_eq!(salt_hex.len(), 32);
    assert_eq!(hash_hex.len(), 64);
    // Flip last hex char of the hash.
    let mut tampered_hash = hash_hex.to_owned();
    let last = tampered_hash.pop().unwrap();
    tampered_hash.push(if last == '0' { '1' } else { '0' });
    let tampered = format!("{salt_hex}${tampered_hash}");
    assert!(consume_backup_code(&tampered, "ABCD-1234").is_none());

    // Malformed entries are skipped.
    assert!(
      consume_backup_code("not-valid, ,no-dollar", "ABCD-1234").is_none()
    );
    assert!(consume_backup_code("", "ABCD-1234").is_none());
  }

  #[test]
  fn reuse_key_is_user_and_code_scoped() {
    let a = uuid::Uuid::now_v7();
    let b = uuid::Uuid::now_v7();
    assert_eq!(
      totp_reuse_key(&a, " 123456 "),
      format!("totp:used:{}:123456", a.as_simple())
    );
    // Different user, same code → different key.
    assert_ne!(totp_reuse_key(&a, "123456"), totp_reuse_key(&b, "123456"));
    // Different code, same user → different key.
    assert_ne!(totp_reuse_key(&a, "123456"), totp_reuse_key(&a, "654321"));
    assert!(totp_reuse_key(&a, "123456").starts_with("totp:used:"));
  }
}
