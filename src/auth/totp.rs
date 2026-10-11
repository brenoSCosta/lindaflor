use argon2::password_hash::{
  PasswordHasher, PasswordVerifier, phc::PasswordHash,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::Rng;
use totp_rs::{Algorithm as TotpAlgorithm, Builder, Secret};

use crate::auth::password::PasswordError;

pub const ISSUER: &str = "lindaflor";
const BACKUP_CODE_COUNT: usize = 10;

/// Generate a new TOTP secret (base32) and otpauth URI for `account` (usually email).
pub fn generate_secret(account: &str) -> Result<(String, String), String> {
  let secret = Secret::generate();
  let secret_b32 = secret.to_base32();
  let totp = Builder::new()
    .with_algorithm(TotpAlgorithm::SHA1)
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
    .with_algorithm(TotpAlgorithm::SHA1)
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

/// Argon2id instance for backup-code hashing.
///
/// Parameters `m=8192 KiB, t=2, p=1`: lighter than [`Argon2::default()`]
/// (`m=19456, t=2, p=1`) on purpose. Hashing consumes 10 codes at
/// enable/regen time and verifying scans up to 10 entries in the worst
/// case, so full-strength defaults would push p99 past what a single
/// `spawn_blocking` slot plus a 5s route timeout comfortably covers —
/// a DoS amplifier. Online guessing is already capped by
/// `guard_pending_2fa` (`PENDING_MAX_ATTEMPTS = 5`), so `m=8192` keeps
/// offline brute force far beyond fast `SHA-256` while staying servable.
/// `p=1` avoids oversubscribing the blocking pool when consumes run
/// concurrently. Verification reads cost params from the PHC string, so
/// this instance also verifies hashes created with different params.
fn backup_argon2() -> Result<Argon2<'static>, PasswordError> {
  let params = Params::new(8192, 2, 1, None)
    .map_err(|e| PasswordError::Hash(e.to_string()))?;
  Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Hash backup codes for storage: newline-joined Argon2id PHC strings.
///
/// Each entry is `$argon2id$...` — entries are newline-joined (PHC params
/// contain commas, so comma-join would corrupt entries on split).
/// Blocking CPU work: callers must run this via
/// `tokio::task::spawn_blocking`. Never log the inputs.
pub fn hash_backup_codes(codes: &[String]) -> Result<String, PasswordError> {
  let argon2 = backup_argon2()?;
  codes
    .iter()
    .map(|code| {
      let normalized = normalize_backup_code(code);
      argon2
        .hash_password(normalized.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::Hash(e.to_string()))
    })
    .collect::<Result<Vec<_>, _>>()
    .map(|entries| entries.join("\n"))
}

/// Consume one matching backup code. Returns remaining stored entries on
/// match, `None` on mismatch.
///
/// Argon2id PHC only: every entry is one `$argon2id$` PHC string per line.
/// Malformed entries are skipped so one bad row never fails the other 9.
/// Blocking CPU work (Argon2): callers must `spawn_blocking` + timeout
/// *after* `guard_pending_2fa`.
pub fn consume_backup_code(stored: &str, presented: &str) -> Option<String> {
  let needle = normalize_backup_code(presented);
  if needle.is_empty() {
    return None;
  }
  let mut entries: Vec<String> = Vec::new();
  for line in stored.split('\n') {
    let line = line.trim();
    if line.is_empty() {
      continue;
    }
    entries.push(line.to_owned());
  }
  for (idx, entry) in entries.iter().enumerate() {
    if !verify_argon2_entry(entry, &needle) {
      continue;
    }
    let mut remaining = entries.clone();
    remaining.remove(idx);
    return Some(remaining.join("\n"));
  }
  None
}

/// Argon2id verify for one PHC entry. `false` on mismatch *and* on any
/// parse/param error (malformed entries are skipped, never fatal).
/// [`Argon2::verify_password`] is constant-time; no manual compare here.
fn verify_argon2_entry(entry: &str, needle: &str) -> bool {
  let parsed = match PasswordHash::new(entry) {
    Ok(parsed) => parsed,
    Err(_) => return false,
  };
  let argon2 = match backup_argon2() {
    Ok(argon2) => argon2,
    Err(_) => return false,
  };
  // Cost params come from the PHC string itself, so entries hashed with
  // different params (e.g. a future upgrade) still verify.
  match argon2.verify_password(needle.as_bytes(), &parsed) {
    Ok(()) => true,
    Err(_) => false,
  }
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
  fn generated_codes_keep_dddd_dddd_format() {
    for code in generate_backup_codes() {
      assert_eq!(code.len(), 9, "unexpected code: {code}");
      let (a, b) = code.split_once('-').expect("dash");
      assert_eq!(a.len(), 4, "unexpected code: {code}");
      assert_eq!(b.len(), 4, "unexpected code: {code}");
      assert!(a.chars().all(|c| c.is_ascii_digit()), "{code}");
      assert!(b.chars().all(|c| c.is_ascii_digit()), "{code}");
    }
  }

  #[test]
  fn stored_entries_are_argon2id_phc() {
    let codes = vec!["ABCD-1234".to_string(), "0000-0001".to_string()];
    let stored = hash_backup_codes(&codes).expect("hash");
    assert!(
      !stored.contains("ABCD"),
      "plaintext must not leak into storage"
    );
    let entries: Vec<&str> = stored.split('\n').collect();
    assert_eq!(entries.len(), 2);
    for entry in entries {
      let entry = entry.trim();
      assert!(entry.starts_with("$argon2id$"), "entry must be PHC");
      assert!(PasswordHash::new(entry).is_ok(), "entry must parse as PHC");
      assert!(!entry.contains('\n'), "entries are newline-separated");
    }
    // PHC params contain commas — entries must survive a comma split
    // only when parsed line-wise, never field-wise.
    assert!(stored.contains(','), "PHC params embed commas");
  }

  #[test]
  fn round_trip_consume_each_once() {
    let codes = generate_backup_codes();
    assert_eq!(codes.len(), 10);
    let mut stored = hash_backup_codes(&codes).expect("hash");
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
    let stored = hash_backup_codes(&codes).expect("hash");
    // lowercase + no dash
    assert!(consume_backup_code(&stored, "abcd1234").is_some());
    assert!(consume_backup_code(&stored, "abcd-1234").is_some());
    assert!(consume_backup_code(&stored, "ABCD1234").is_some());
    assert!(consume_backup_code(&stored, "  abcd-1234  ").is_some());
  }

  #[test]
  fn wrong_code_returns_none() {
    let codes = vec!["ABCD-1234".to_string()];
    let stored = hash_backup_codes(&codes).expect("hash");
    assert!(consume_backup_code(&stored, "ZZZZ-9999").is_none());
    assert!(consume_backup_code(&stored, "").is_none());
    assert!(consume_backup_code(&stored, "---").is_none());
    // Stored must be unchanged (still consumable with right code).
    assert!(consume_backup_code(&stored, "ABCD-1234").is_some());
  }

  #[test]
  fn tampered_phc_does_not_match() {
    let codes = vec!["ABCD-1234".to_string()];
    let stored = hash_backup_codes(&codes).expect("hash");
    assert!(stored.starts_with("$argon2id$"));
    // Flip last char of the PHC string.
    let mut tampered = stored.clone();
    let last = tampered.pop().expect("non-empty");
    tampered.push(if last == 'A' { 'B' } else { 'A' });
    assert!(consume_backup_code(&tampered, "ABCD-1234").is_none());
    // Untouched entry still verifies.
    assert!(consume_backup_code(&stored, "ABCD-1234").is_some());
  }

  #[test]
  fn malformed_entries_are_skipped_not_fatal() {
    let codes = vec!["ABCD-1234".to_string()];
    let stored = hash_backup_codes(&codes).expect("hash");
    // Garbage-only stores never match (and never panic).
    assert!(consume_backup_code("not-valid\nno-dollar", "ABCD-1234").is_none());
    assert!(consume_backup_code("", "ABCD-1234").is_none());
    // A truncated PHC entry is skipped, not fatal.
    assert!(
      consume_backup_code("$argon2id$v=19$m=8192,t=2,p=1,broken", "ABCD-1234")
        .is_none()
    );
    // One valid entry among garbage still consumes.
    let mixed = format!("garbage-no-dollar\n{stored}\n   \n$argon2id$broken");
    let remaining =
      consume_backup_code(&mixed, "ABCD-1234").expect("valid entry matches");
    // Matched PHC entry removed; garbage + truncated PHC preserved.
    let lines: Vec<&str> = remaining.lines().collect();
    assert_eq!(lines, vec!["garbage-no-dollar", "$argon2id$broken"]);
  }

  #[test]
  fn non_phc_entries_never_verify() {
    // PHC-only store: hex$salt-style rows and comma-joined fragments are
    // just malformed lines — they never match and never corrupt parsing.
    let stored = hash_backup_codes(&["ABCD-1234".to_string()]).expect("hash");
    assert!(
      consume_backup_code(
        "00112233445566778899aabbccddeeff$fc21e2dc2665f7e70258365faaac64fd04b9d327395956095b4bfdc4982a492d",
        "1234-5678"
      )
      .is_none()
    );
    assert!(consume_backup_code(&stored, "ZZZZ-9999").is_none());
    // Commas are data (PHC params), never a separator: a comma-joined
    // rewrite of a valid store must not verify.
    let comma_joined = stored.replace('\n', ",");
    if comma_joined != stored {
      assert!(consume_backup_code(&comma_joined, "ABCD-1234").is_none());
    }
  }

  #[test]
  fn empty_store_or_empty_code_returns_none() {
    let codes = vec!["ABCD-1234".to_string()];
    let stored = hash_backup_codes(&codes).expect("hash");
    assert!(consume_backup_code("", "ABCD-1234").is_none());
    assert!(consume_backup_code("\n", "ABCD-1234").is_none());
    assert!(consume_backup_code(&stored, "").is_none());
    assert!(consume_backup_code(&stored, "   ").is_none());
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
