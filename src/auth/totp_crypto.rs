//! XChaCha20-Poly1305 encryption for TOTP secrets at rest.
//!
//! `two_factor` stores `secret_nonce` (24-byte XChaCha nonce) +
//! `secret_ciphertext` (AEAD of the base32 secret) + `secret_key_version`
//! (all NOT NULL — see `migrations/0009_totp_secret_encrypted.sql`). There
//! is no plaintext column and no legacy path. The key comes from
//! [`crate::config::totp_encryption_key`] (KMS-wrapped env key); this module
//! is the only place that touches AEAD types or key bytes. Routes and
//! services only ever see [`Zeroizing`]`<String>` plaintexts, which are
//! zeroed on drop.
//!
//! Every decrypt failure (wrong key, tampered nonce/tag, version mismatch)
//! fails hard. Never log secrets or key material; error messages carry no
//! secret bytes.

use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce, aead::Aead};
use rand::RngCore;
use zeroize::Zeroizing;

use crate::config::{ConfigError, totp_encryption_key};

/// Current `two_factor.secret_key_version`. Bump when rotating to a new
/// `TOTP_ENCRYPTION_KEY` so old ciphertexts can be distinguished.
pub const TOTP_SECRET_KEY_VERSION: i16 = 1;

/// XChaCha nonce length in bytes.
pub const TOTP_NONCE_LEN: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum TotpCryptoError {
  #[error("totp encryption key is not configured: {0}")]
  Key(#[from] ConfigError),
  #[error(
    "unsupported totp secret key version {0} (expected {TOTP_SECRET_KEY_VERSION})"
  )]
  UnsupportedVersion(i16),
  #[error("totp secret nonce must be {TOTP_NONCE_LEN} bytes")]
  BadNonce,
  #[error("totp secret authentication failed")]
  DecryptFailed,
  #[error("decrypted totp secret is not valid UTF-8")]
  InvalidUtf8,
}

/// Freshly encrypted TOTP secret, ready for INSERT.
///
/// Manual `Debug`: ciphertext length only, never bytes.
pub struct EncryptedTotp {
  pub nonce: Vec<u8>,
  pub ciphertext: Vec<u8>,
  pub key_version: i16,
}

impl std::fmt::Debug for EncryptedTotp {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("EncryptedTotp")
      .field("nonce_len", &self.nonce.len())
      .field("ciphertext_len", &self.ciphertext.len())
      .field("key_version", &self.key_version)
      .finish()
  }
}

/// Encrypt a base32 TOTP secret with the configured key and a fresh random
/// 24-byte nonce. Blocking? No — XChaCha20-Poly1305 on ~32B is microseconds;
/// callers may run this inline. Never log the input.
pub fn encrypt_secret(
  plaintext_b32: &str,
) -> Result<EncryptedTotp, TotpCryptoError> {
  let key = totp_encryption_key()?;
  Ok(encrypt_with_key(plaintext_b32, &key))
}

/// Decrypt (`secret_nonce`, `secret_ciphertext`, `secret_key_version`) into
/// a zeroizing plaintext. Columns are NOT NULL: callers pass the decoded
/// values directly. Fails hard on wrong key, tampered nonce/tag, or
/// version mismatch. Never log inputs or outputs.
pub fn decrypt_secret(
  nonce: &[u8],
  ciphertext: &[u8],
  version: i16,
) -> Result<Zeroizing<String>, TotpCryptoError> {
  let key = totp_encryption_key()?;
  decrypt_with_key(nonce, ciphertext, version, &key)
}

/// Decrypt one `two_factor` row. All three columns are NOT NULL, so the
/// arguments are required — a missing value is a caller bug and fails hard.
/// There is no plaintext fallback.
pub fn decrypt_row_secret(
  nonce: Vec<u8>,
  ciphertext: Vec<u8>,
  version: i16,
) -> Result<Zeroizing<String>, TotpCryptoError> {
  decrypt_secret(&nonce, &ciphertext, version)
}

fn aead(key_bytes: &[u8; 32]) -> XChaCha20Poly1305 {
  XChaCha20Poly1305::new(Key::from_slice(key_bytes))
}

fn encrypt_with_key(
  plaintext_b32: &str,
  key_bytes: &[u8; 32],
) -> EncryptedTotp {
  let mut nonce_bytes = [0u8; TOTP_NONCE_LEN];
  rand::thread_rng().fill_bytes(&mut nonce_bytes);
  let ciphertext = aead(key_bytes)
    .encrypt(XNonce::from_slice(&nonce_bytes), plaintext_b32.as_bytes())
    .expect("xchacha encryption with a valid key and fresh nonce");
  EncryptedTotp {
    nonce: nonce_bytes.to_vec(),
    ciphertext,
    key_version: TOTP_SECRET_KEY_VERSION,
  }
}

fn decrypt_with_key(
  nonce: &[u8],
  ciphertext: &[u8],
  version: i16,
  key_bytes: &[u8; 32],
) -> Result<Zeroizing<String>, TotpCryptoError> {
  if version != TOTP_SECRET_KEY_VERSION {
    return Err(TotpCryptoError::UnsupportedVersion(version));
  }
  if nonce.len() != TOTP_NONCE_LEN {
    return Err(TotpCryptoError::BadNonce);
  }
  let plaintext = aead(key_bytes)
    .decrypt(XNonce::from_slice(nonce), ciphertext)
    .map_err(|_| TotpCryptoError::DecryptFailed)?;
  let text =
    String::from_utf8(plaintext).map_err(|_| TotpCryptoError::InvalidUtf8)?;
  Ok(Zeroizing::new(text))
}

#[cfg(test)]
mod tests {
  use super::*;

  const KEY_A: [u8; 32] = [0x11; 32];
  const KEY_B: [u8; 32] = [0x22; 32];
  const SECRET_B32: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";

  #[test]
  fn round_trip_recovers_secret() {
    let enc = encrypt_with_key(SECRET_B32, &KEY_A);
    assert_eq!(enc.nonce.len(), TOTP_NONCE_LEN);
    assert_eq!(enc.key_version, TOTP_SECRET_KEY_VERSION);
    assert!(!enc.ciphertext.is_empty());
    assert_ne!(enc.ciphertext, SECRET_B32.as_bytes());
    let dec =
      decrypt_with_key(&enc.nonce, &enc.ciphertext, enc.key_version, &KEY_A)
        .expect("decrypt");
    assert_eq!(dec.as_str(), SECRET_B32);
  }

  #[test]
  fn nonces_are_random_per_encryption() {
    let a = encrypt_with_key(SECRET_B32, &KEY_A);
    let b = encrypt_with_key(SECRET_B32, &KEY_A);
    assert_ne!(a.nonce, b.nonce);
    assert_ne!(a.ciphertext, b.ciphertext);
  }

  #[test]
  fn wrong_key_fails_closed() {
    let enc = encrypt_with_key(SECRET_B32, &KEY_A);
    let err =
      decrypt_with_key(&enc.nonce, &enc.ciphertext, enc.key_version, &KEY_B)
        .expect_err("wrong key must fail");
    assert!(matches!(err, TotpCryptoError::DecryptFailed));
  }

  #[test]
  fn tampered_nonce_or_tag_fails_closed() {
    let enc = encrypt_with_key(SECRET_B32, &KEY_A);
    let mut nonce = enc.nonce.clone();
    nonce[0] ^= 0x01;
    assert!(matches!(
      decrypt_with_key(&nonce, &enc.ciphertext, enc.key_version, &KEY_A),
      Err(TotpCryptoError::DecryptFailed)
    ));
    let mut tag = enc.ciphertext.clone();
    let last = tag.len() - 1;
    tag[last] ^= 0x01;
    assert!(matches!(
      decrypt_with_key(&enc.nonce, &tag, enc.key_version, &KEY_A),
      Err(TotpCryptoError::DecryptFailed)
    ));
  }

  #[test]
  fn short_nonce_is_rejected() {
    let enc = encrypt_with_key(SECRET_B32, &KEY_A);
    assert!(matches!(
      decrypt_with_key(
        &enc.nonce[..12],
        &enc.ciphertext,
        enc.key_version,
        &KEY_A
      ),
      Err(TotpCryptoError::BadNonce)
    ));
  }

  #[test]
  fn version_mismatch_is_rejected() {
    let enc = encrypt_with_key(SECRET_B32, &KEY_A);
    let err = decrypt_with_key(
      &enc.nonce,
      &enc.ciphertext,
      TOTP_SECRET_KEY_VERSION + 1,
      &KEY_A,
    )
    .expect_err("version mismatch must fail");
    assert!(matches!(err, TotpCryptoError::UnsupportedVersion(_)));
  }

  #[test]
  fn errors_and_debug_carry_no_secret_material() {
    let enc = encrypt_with_key(SECRET_B32, &KEY_A);
    let rendered = format!("{enc:?}");
    assert!(!rendered.contains(SECRET_B32), "leak: {rendered}");
    for err in [
      TotpCryptoError::DecryptFailed,
      TotpCryptoError::BadNonce,
      TotpCryptoError::InvalidUtf8,
      TotpCryptoError::UnsupportedVersion(9),
    ] {
      let rendered = format!("{err}");
      assert!(!rendered.contains(SECRET_B32), "leak: {rendered}");
    }
    // A failed decrypt never yields the plaintext.
    let err =
      decrypt_with_key(&enc.nonce, &enc.ciphertext, enc.key_version, &KEY_B)
        .expect_err("must fail");
    assert!(!format!("{err:?}").contains(SECRET_B32));
  }
}
