//! Mercado Pago webhook `x-signature` verification.
//!
//! Header: `x-signature: ts=<unix>,v1=<hex hmac>`. The signed manifest is
//! `id:{data.id};request-id:{x-request-id};ts:{ts};` where `data.id` comes
//! from the notification URL's query string (lowercased when alphanumeric)
//! and any missing part is omitted. `v1 = hex(HMAC_SHA256(secret, manifest))`.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// How far `ts` may drift from our clock before the notification is
/// rejected as stale (replay window).
pub const MAX_SKEW_SECS: i64 = 300;

#[derive(Debug, PartialEq, Eq)]
pub enum SignatureError {
  Missing,
  Malformed,
  Stale,
  Mismatch,
}

struct ParsedSignature<'a> {
  ts: &'a str,
  v1: &'a str,
}

fn parse_header(header: &str) -> Result<ParsedSignature<'_>, SignatureError> {
  let mut ts = None;
  let mut v1 = None;
  for part in header.split(',') {
    let Some((key, value)) = part.split_once('=') else {
      continue;
    };
    match key.trim() {
      "ts" => ts = Some(value.trim()),
      "v1" => v1 = Some(value.trim()),
      _ => {}
    }
  }
  match (ts, v1) {
    (Some(ts), Some(v1)) if !ts.is_empty() && !v1.is_empty() => {
      Ok(ParsedSignature { ts, v1 })
    }
    _ => Err(SignatureError::Malformed),
  }
}

/// Build the manifest MP signs. Alphanumeric ids are lowercased (MP docs);
/// absent parts are left out.
pub fn manifest(
  data_id: Option<&str>,
  request_id: Option<&str>,
  ts: &str,
) -> String {
  let mut out = String::new();
  if let Some(id) = data_id.filter(|id| !id.is_empty()) {
    let id = if id.chars().all(|c| c.is_ascii_alphanumeric()) {
      id.to_ascii_lowercase()
    } else {
      id.to_string()
    };
    out.push_str(&format!("id:{id};"));
  }
  if let Some(request_id) = request_id.filter(|r| !r.is_empty()) {
    out.push_str(&format!("request-id:{request_id};"));
  }
  out.push_str(&format!("ts:{ts};"));
  out
}

/// Hex HMAC-SHA256 of `manifest` (also used by tests to sign fixtures).
pub fn sign(secret: &str, manifest: &str) -> String {
  let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
    .expect("hmac accepts any key");
  mac.update(manifest.as_bytes());
  hex::encode(mac.finalize().into_bytes())
}

/// Verify an `x-signature` header. `now_secs` is the current Unix time.
/// `ts` is accepted in seconds or milliseconds.
pub fn verify(
  secret: &str,
  header: Option<&str>,
  request_id: Option<&str>,
  data_id: Option<&str>,
  now_secs: i64,
) -> Result<(), SignatureError> {
  if secret.is_empty() {
    return Err(SignatureError::Missing);
  }
  let header = header.ok_or(SignatureError::Missing)?;
  let parsed = parse_header(header)?;
  let ts: i64 = parsed.ts.parse().map_err(|_| SignatureError::Malformed)?;
  let ts_secs = if ts > 100_000_000_000 { ts / 1000 } else { ts };
  if (now_secs - ts_secs).abs() > MAX_SKEW_SECS {
    return Err(SignatureError::Stale);
  }
  let expected =
    hex::decode(parsed.v1).map_err(|_| SignatureError::Mismatch)?;
  let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
    .expect("hmac accepts any key");
  mac.update(manifest(data_id, request_id, parsed.ts).as_bytes());
  // Constant-time comparison.
  mac
    .verify_slice(&expected)
    .map_err(|_| SignatureError::Mismatch)
}

#[cfg(test)]
mod tests {
  use super::*;

  const SECRET: &str = "test-webhook-secret";
  const NOW: i64 = 1_790_000_000;

  fn header_for(data_id: &str, request_id: &str, ts: i64) -> String {
    let ts = ts.to_string();
    let v1 = sign(SECRET, &manifest(Some(data_id), Some(request_id), &ts));
    format!("ts={ts},v1={v1}")
  }

  #[test]
  fn manifest_matches_mercadopago_template() {
    assert_eq!(
      manifest(Some("123456"), Some("req-1"), "1704908010"),
      "id:123456;request-id:req-1;ts:1704908010;"
    );
    assert_eq!(
      manifest(Some("ABC123"), None, "1"),
      "id:abc123;ts:1;",
      "alphanumeric ids are lowercased; missing parts omitted"
    );
  }

  #[test]
  fn valid_signature_passes() {
    let header = header_for("987", "req-abc", NOW);
    assert_eq!(
      verify(
        SECRET,
        Some(&header),
        Some("req-abc"),
        Some("987"),
        NOW + 10
      ),
      Ok(())
    );
    // Header parts may be spaced / reordered.
    let v1 = sign(SECRET, &manifest(Some("987"), Some("r"), &NOW.to_string()));
    let header = format!("v1={v1}, ts={NOW}");
    assert_eq!(
      verify(SECRET, Some(&header), Some("r"), Some("987"), NOW),
      Ok(())
    );
  }

  #[test]
  fn millisecond_ts_is_accepted() {
    let ts_ms = NOW * 1000;
    let header = header_for("5", "r", ts_ms);
    assert_eq!(
      verify(SECRET, Some(&header), Some("r"), Some("5"), NOW),
      Ok(())
    );
  }

  #[test]
  fn invalid_signatures_are_rejected() {
    let header = header_for("987", "req-abc", NOW);
    // Different payment id than the one signed.
    assert_eq!(
      verify(SECRET, Some(&header), Some("req-abc"), Some("988"), NOW),
      Err(SignatureError::Mismatch)
    );
    // Different request id.
    assert_eq!(
      verify(SECRET, Some(&header), Some("other"), Some("987"), NOW),
      Err(SignatureError::Mismatch)
    );
    // Wrong secret.
    assert_eq!(
      verify("nope", Some(&header), Some("req-abc"), Some("987"), NOW),
      Err(SignatureError::Mismatch)
    );
    assert_eq!(
      verify(SECRET, None, Some("req-abc"), Some("987"), NOW),
      Err(SignatureError::Missing)
    );
    assert_eq!(
      verify(SECRET, Some("garbage"), None, Some("987"), NOW),
      Err(SignatureError::Malformed)
    );
    assert_eq!(
      verify(SECRET, Some("ts=1,v1=zz"), None, Some("987"), 1),
      Err(SignatureError::Mismatch)
    );
  }

  #[test]
  fn stale_timestamps_are_rejected() {
    let old = header_for("987", "r", NOW - MAX_SKEW_SECS - 1);
    assert_eq!(
      verify(SECRET, Some(&old), Some("r"), Some("987"), NOW),
      Err(SignatureError::Stale)
    );
    let future = header_for("987", "r", NOW + MAX_SKEW_SECS + 1);
    assert_eq!(
      verify(SECRET, Some(&future), Some("r"), Some("987"), NOW),
      Err(SignatureError::Stale)
    );
  }
}
