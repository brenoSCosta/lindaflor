use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

const PRESIGN_EXPIRY_SECS: u64 = 86_400;

#[derive(Clone)]
pub struct S3ObjectStore {
  http: reqwest::Client,
  endpoint: String,
  bucket: String,
  region: String,
  access_key_id: String,
  secret_access_key: String,
}

impl S3ObjectStore {
  fn new(config: &crate::config::S3Config) -> Self {
    Self {
      http: reqwest::Client::new(),
      endpoint: config.endpoint.trim_end_matches('/').to_string(),
      bucket: config.bucket.clone(),
      region: config.region.clone(),
      access_key_id: config.access_key_id.clone(),
      secret_access_key: config.secret_access_key.clone(),
    }
  }

  async fn put(
    &self,
    key: &str,
    body: Vec<u8>,
    content_type: &str,
  ) -> Result<(), StorageError> {
    let (amz_date, date_stamp) = amz_timestamps();
    let payload_hash = sha256_hex(&body);
    let canonical_uri = self.object_canonical_uri(key);
    let auth = self.authorization(
      "PUT",
      &canonical_uri,
      "",
      &payload_hash,
      &amz_date,
      &date_stamp,
    )?;
    let response = self
      .http
      .put(self.object_url(key))
      .header("host", self.host())
      .header("x-amz-content-sha256", payload_hash)
      .header("x-amz-date", amz_date)
      .header(reqwest::header::CONTENT_TYPE, content_type)
      .header(reqwest::header::CONTENT_LENGTH, body.len())
      .header(reqwest::header::AUTHORIZATION, auth)
      .body(body)
      .send()
      .await
      .map_err(StorageError::from_aws)?;
    if response.status().is_success() {
      Ok(())
    } else {
      Err(status_error(response).await)
    }
  }

  async fn delete(&self, key: &str) -> Result<(), StorageError> {
    let (amz_date, date_stamp) = amz_timestamps();
    let payload_hash = sha256_hex(&[]);
    let canonical_uri = self.object_canonical_uri(key);
    let auth = self.authorization(
      "DELETE",
      &canonical_uri,
      "",
      &payload_hash,
      &amz_date,
      &date_stamp,
    )?;
    let response = self
      .http
      .delete(self.object_url(key))
      .header("host", self.host())
      .header("x-amz-content-sha256", payload_hash)
      .header("x-amz-date", amz_date)
      .header(reqwest::header::AUTHORIZATION, auth)
      .send()
      .await
      .map_err(StorageError::from_aws)?;
    match response.status().as_u16() {
      // Idempotent delete: a missing key is not an error.
      200 | 204 | 404 => Ok(()),
      _ => Err(status_error(response).await),
    }
  }

  async fn presign_get(&self, key: &str) -> Result<String, StorageError> {
    let (amz_date, date_stamp) = amz_timestamps();
    self.presigned_url(key, &amz_date, &date_stamp)
  }

  async fn ensure_bucket(&self) -> Result<(), StorageError> {
    let (amz_date, date_stamp) = amz_timestamps();
    let payload_hash = sha256_hex(&[]);
    let canonical_uri = self.bucket_canonical_uri();
    let auth = self.authorization(
      "HEAD",
      &canonical_uri,
      "",
      &payload_hash,
      &amz_date,
      &date_stamp,
    )?;
    let response = self
      .http
      .head(self.bucket_url())
      .header("host", self.host())
      .header("x-amz-content-sha256", payload_hash)
      .header("x-amz-date", amz_date)
      .header(reqwest::header::AUTHORIZATION, auth)
      .send()
      .await
      .map_err(StorageError::from_aws)?;
    let status = response.status();
    if status.as_u16() == 200 {
      return Ok(());
    }
    // HEAD responses carry no body, but check it anyway for NoSuchBucket
    // so S3-likes that surface the code this way still trigger creation.
    let body = response.text().await.unwrap_or_default();
    if matches!(status.as_u16(), 403 | 404) || body.contains("NoSuchBucket") {
      self.create_bucket().await
    } else {
      Err(status_error_with_body(status, &body))
    }
  }

  async fn create_bucket(&self) -> Result<(), StorageError> {
    // AWS rejects a LocationConstraint in us-east-1; other regions require one.
    let is_us_east_1 = self.region.eq_ignore_ascii_case("us-east-1");
    let body: Vec<u8> = if is_us_east_1 {
      Vec::new()
    } else {
      format!(
        "<CreateBucketConfiguration>\
         <LocationConstraint>{}</LocationConstraint>\
         </CreateBucketConfiguration>",
        self.region
      )
      .into_bytes()
    };
    let (amz_date, date_stamp) = amz_timestamps();
    let payload_hash = sha256_hex(&body);
    let canonical_uri = self.bucket_canonical_uri();
    let auth = self.authorization(
      "PUT",
      &canonical_uri,
      "",
      &payload_hash,
      &amz_date,
      &date_stamp,
    )?;
    let mut req = self
      .http
      .put(self.bucket_url())
      .header("host", self.host())
      .header("x-amz-content-sha256", payload_hash)
      .header("x-amz-date", amz_date)
      .header(reqwest::header::AUTHORIZATION, auth);
    if !is_us_east_1 {
      req = req.header(reqwest::header::CONTENT_TYPE, "application/xml");
    }
    let response =
      req.body(body).send().await.map_err(StorageError::from_aws)?;
    if response.status().is_success() {
      Ok(())
    } else {
      Err(status_error(response).await)
    }
  }

  fn object_url(&self, key: &str) -> String {
    format!("{}/{}/{}", self.endpoint, self.bucket, encode_path(key))
  }

  fn bucket_url(&self) -> String {
    format!("{}/{}", self.endpoint, self.bucket)
  }

  fn object_canonical_uri(&self, key: &str) -> String {
    format!("/{}/{}", self.bucket, encode_path(key))
  }

  fn bucket_canonical_uri(&self) -> String {
    format!("/{}", self.bucket)
  }

  fn host(&self) -> String {
    signed_host(&self.endpoint)
  }

  fn credential_scope(&self, date_stamp: &str) -> String {
    format!("{date_stamp}/{}/s3/aws4_request", self.region)
  }

  /// SigV4 `Authorization` header value for a header-signed request.
  /// Signs exactly the headers sent: `host`, `x-amz-content-sha256`,
  /// `x-amz-date` (plus `content-type`/`content-length` transmitted
  /// unsigned on PUT, as S3 requires them on the wire but not in the
  /// signature for this client).
  fn authorization(
    &self,
    method: &str,
    canonical_uri: &str,
    query_string: &str,
    payload_hash: &str,
    amz_date: &str,
    date_stamp: &str,
  ) -> Result<String, StorageError> {
    let host = self.host();
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_headers = format!(
      "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let canonical_request = format!(
      "{method}\n{canonical_uri}\n{query_string}\n\
       {canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let scope = self.credential_scope(date_stamp);
    let string_to_sign = format!(
      "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
      sha256_hex(canonical_request.as_bytes())
    );
    let signing_key =
      derive_signing_key(&self.secret_access_key, date_stamp, &self.region)?;
    let signature = hex_hmac(&signing_key, string_to_sign.as_bytes())?;
    Ok(format!(
      "AWS4-HMAC-SHA256 Credential={}/{scope}, \
       SignedHeaders={signed_headers}, Signature={signature}",
      self.access_key_id
    ))
  }

  /// Pure presigned-GET URL construction (no HTTP call).
  fn presigned_url(
    &self,
    key: &str,
    amz_date: &str,
    date_stamp: &str,
  ) -> Result<String, StorageError> {
    let host = self.host();
    let canonical_uri = self.object_canonical_uri(key);
    let scope = self.credential_scope(date_stamp);
    let credential = format!("{}/{}", self.access_key_id, scope);
    // Query params in ascending name order (required for signing).
    let canonical_qs = format!(
      "X-Amz-Algorithm=AWS4-HMAC-SHA256\
       &X-Amz-Credential={}\
       &X-Amz-Date={amz_date}\
       &X-Amz-Expires={PRESIGN_EXPIRY_SECS}\
       &X-Amz-SignedHeaders=host",
      encode_query_value(&credential),
    );
    let canonical_request = format!(
      "GET\n{canonical_uri}\n{canonical_qs}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD"
    );
    let string_to_sign = format!(
      "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
      sha256_hex(canonical_request.as_bytes())
    );
    let signing_key =
      derive_signing_key(&self.secret_access_key, date_stamp, &self.region)?;
    let signature = hex_hmac(&signing_key, string_to_sign.as_bytes())?;
    Ok(format!(
      "{}/{}/{canonical_uri_suffix}?{canonical_qs}&X-Amz-Signature={signature}",
      self.endpoint,
      self.bucket,
      canonical_uri_suffix = encode_path(key),
    ))
  }
}

fn is_unreserved(byte: u8) -> bool {
  matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
}

/// AWS percent-encoding over the UTF-8 bytes of `segment`: unreserved
/// bytes verbatim, everything else `%XX` with uppercase hex.
fn percent_encode_segment(segment: &str) -> String {
  let mut out = String::with_capacity(segment.len());
  for byte in segment.as_bytes() {
    if is_unreserved(*byte) {
      out.push(*byte as char);
    } else {
      out.push_str(&format!("%{byte:02X}"));
    }
  }
  out
}

/// Encode an object key for a URL path: each `/`-separated segment is
/// encoded and `/` is preserved as the separator.
fn encode_path(key: &str) -> String {
  key.split('/').map(percent_encode_segment).collect::<Vec<_>>().join("/")
}

/// Encode a value for a SigV4 query string: same rules as path segments
/// except `/` is encoded as `%2F`.
fn encode_query_value(value: &str) -> String {
  percent_encode_segment(value).replace('/', "%2F")
}

/// Value to sign (and send) as the `host` header, derived from the
/// endpoint URL, including the port when non-default.
/// `Url::port()` returns `None` for default ports, so `Some` implies
/// non-default here.
fn signed_host(endpoint: &str) -> String {
  if let Ok(url) = reqwest::Url::parse(endpoint)
    && let Some(host) = url.host_str()
  {
    match url.port() {
      Some(port) => format!("{host}:{port}"),
      None => host.to_string(),
    }
  } else {
    // Best-effort fallback for a non-URL endpoint: strip the scheme and
    // take the authority component.
    let without_scheme = endpoint.split("://").last().unwrap_or(endpoint);
    without_scheme.split('/').next().unwrap_or(without_scheme).to_string()
  }
}

/// `(amz_date, date_stamp)` as `YYYYMMDDTHHMMSSZ` + `YYYYMMDD`,
/// formatted manually from components (no `time` formatting feature).
fn amz_timestamps() -> (String, String) {
  let now = time::OffsetDateTime::now_utc();
  let amz_date = format!(
    "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
    now.year(),
    u8::from(now.month()),
    now.day(),
    now.hour(),
    now.minute(),
    now.second(),
  );
  let date_stamp = amz_date[..8].to_string();
  (amz_date, date_stamp)
}

fn sha256_hex(data: &[u8]) -> String {
  hex::encode(Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<[u8; 32], StorageError> {
  let bytes = Hmac::<Sha256>::new_from_slice(key)
    .map_err(StorageError::from_aws)?
    .chain_update(data)
    .finalize()
    .into_bytes();
  let mut out = [0u8; 32];
  out.copy_from_slice(&bytes);
  Ok(out)
}

/// SigV4 key chain: `AWS4{secret} -> date -> region -> "s3" ->
/// "aws4_request"`.
fn derive_signing_key(
  secret: &str,
  date_stamp: &str,
  region: &str,
) -> Result<Vec<u8>, StorageError> {
  let k_date =
    hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes())?;
  let k_region = hmac_sha256(&k_date, region.as_bytes())?;
  let k_service = hmac_sha256(&k_region, b"s3")?;
  let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
  Ok(k_signing.to_vec())
}

fn hex_hmac(key: &[u8], msg: &[u8]) -> Result<String, StorageError> {
  Ok(hex::encode(hmac_sha256(key, msg)?))
}

async fn status_error(response: reqwest::Response) -> StorageError {
  let status = response.status();
  let body = response.text().await.unwrap_or_default();
  status_error_with_body(status, &body)
}

fn status_error_with_body(
  status: reqwest::StatusCode,
  body: &str,
) -> StorageError {
  let snippet: String = body.chars().take(500).collect();
  StorageError::Other(format!("s3 request failed: status {status} body {snippet}"))
}

type StoredObject = (String, Vec<u8>);
type ObjectMap = HashMap<String, StoredObject>;

#[derive(Clone)]
pub struct MemoryObjectStore {
  objects: Arc<Mutex<ObjectMap>>,
}

impl MemoryObjectStore {
  fn new() -> Self {
    Self {
      objects: Arc::new(Mutex::new(HashMap::new())),
    }
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, ObjectMap> {
    self.objects.lock().unwrap_or_else(|err| err.into_inner())
  }

  async fn put(
    &self,
    key: &str,
    body: Vec<u8>,
    content_type: &str,
  ) -> Result<(), StorageError> {
    self
      .lock()
      .insert(key.to_string(), (content_type.to_string(), body));
    Ok(())
  }

  async fn delete(&self, key: &str) -> Result<(), StorageError> {
    self.lock().remove(key);
    Ok(())
  }

  async fn presign_get(&self, key: &str) -> Result<String, StorageError> {
    if self.lock().contains_key(key) {
      Ok(format!("memory://{key}"))
    } else {
      Err(StorageError::NotFound)
    }
  }
}

#[derive(Clone)]
pub enum ObjectStore {
  S3(S3ObjectStore),
  Memory(MemoryObjectStore),
  Unavailable,
}

impl ObjectStore {
  pub fn unavailable() -> Self {
    Self::Unavailable
  }

  pub fn s3(config: &crate::config::S3Config) -> Self {
    Self::S3(S3ObjectStore::new(config))
  }

  pub fn memory() -> Self {
    Self::Memory(MemoryObjectStore::new())
  }

  pub async fn put(
    &self,
    key: &str,
    body: Vec<u8>,
    content_type: &str,
  ) -> Result<(), StorageError> {
    match self {
      Self::S3(store) => store.put(key, body, content_type).await,
      Self::Memory(store) => store.put(key, body, content_type).await,
      Self::Unavailable => Err(StorageError::Unavailable),
    }
  }

  pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
    match self {
      Self::S3(store) => store.delete(key).await,
      Self::Memory(store) => store.delete(key).await,
      Self::Unavailable => Err(StorageError::Unavailable),
    }
  }

  pub async fn presign_get(&self, key: &str) -> Result<String, StorageError> {
    match self {
      Self::S3(store) => store.presign_get(key).await,
      Self::Memory(store) => store.presign_get(key).await,
      Self::Unavailable => Err(StorageError::Unavailable),
    }
  }

  /// HeadBucket; CreateBucket if missing. No-op for Memory and Unavailable.
  pub async fn ensure_bucket(&self) -> Result<(), StorageError> {
    match self {
      Self::S3(store) => store.ensure_bucket().await,
      Self::Memory(_) | Self::Unavailable => Ok(()),
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
  #[error("object storage is unavailable")]
  Unavailable,
  #[error("object not found")]
  NotFound,
  #[error("object storage error: {0}")]
  Other(String),
}

impl StorageError {
  fn from_aws(err: impl std::fmt::Display) -> Self {
    Self::Other(err.to_string())
  }
}

const _: fn() = || {
  fn assert_send_sync<T: Send + Sync>() {}
  assert_send_sync::<ObjectStore>();
};

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn memory_put_presign_delete() {
    let store = ObjectStore::memory();
    store
      .put("avatars/a.png", b"png-bytes".to_vec(), "image/png")
      .await
      .unwrap();

    let url = store.presign_get("avatars/a.png").await.unwrap();
    assert_eq!(url, "memory://avatars/a.png");

    let cloned = store.clone();
    assert_eq!(
      cloned.presign_get("avatars/a.png").await.unwrap(),
      "memory://avatars/a.png"
    );

    store.delete("avatars/a.png").await.unwrap();
    let missing = store.presign_get("avatars/a.png").await;
    assert!(matches!(missing, Err(StorageError::NotFound)));
  }

  #[test]
  fn s3_encode_unreserved_passthrough() {
    assert_eq!(
      percent_encode_segment("abcXYZ019-_.~"),
      "abcXYZ019-_.~"
    );
    assert_eq!(encode_path("abcXYZ019-_.~"), "abcXYZ019-_.~");
    assert_eq!(encode_query_value("abcXYZ019-_.~"), "abcXYZ019-_.~");
  }

  #[test]
  fn s3_encode_path_preserves_slash_but_query_encodes_it() {
    assert_eq!(encode_path("a/b/c"), "a/b/c");
    assert_eq!(encode_path("avatars//x.jpg"), "avatars//x.jpg");
    assert_eq!(encode_query_value("a/b/c"), "a%2Fb%2Fc");
    assert_eq!(encode_query_value("a/b"), "a%2Fb");
  }

  #[test]
  fn s3_encode_space_unicode_tilde() {
    assert_eq!(encode_path("a b"), "a%20b");
    assert_eq!(encode_path("~"), "~");
    // é = U+00E9 = UTF-8 0xC3 0xA9.
    assert_eq!(encode_path("caf\u{e9}"), "caf%C3%A9");
    // U+1F600 = UTF-8 F0 9F 98 80.
    assert_eq!(encode_path("\u{1f600}"), "%F0%9F%98%80");
    assert_eq!(encode_path("a+b=c&d?e"), "a%2Bb%3Dc%26d%3Fe");
  }

  #[test]
  fn s3_signing_is_deterministic_lowercase_hex() {
    let key =
      derive_signing_key("testsecret", "20240101", "us-east-1").unwrap();
    let msg = b"AWS4-HMAC-SHA256\n20240101T000000Z\n20240101/us-east-1/s3/aws4_request\ndeadbeef";
    let first = hex_hmac(&key, msg).unwrap();
    let second = hex_hmac(&key, msg).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    assert!(
      first.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
      "signature must be 64-char lowercase hex: {first}"
    );
    // A different secret must change the signature.
    let other =
      derive_signing_key("othersecret", "20240101", "us-east-1").unwrap();
    assert_ne!(first, hex_hmac(&other, msg).unwrap());
  }

  #[test]
  fn s3_signed_host_includes_non_default_port() {
    assert_eq!(signed_host("http://localhost:9000"), "localhost:9000");
    assert_eq!(
      signed_host("http://localhost:9000/"),
      "localhost:9000"
    );
    assert_eq!(signed_host("https://s3.amazonaws.com"), "s3.amazonaws.com");
    // Default ports are omitted (Url normalizes them away).
    assert_eq!(signed_host("http://example.com:80"), "example.com");
    assert_eq!(signed_host("https://example.com:443"), "example.com");
  }

  fn test_store() -> S3ObjectStore {
    S3ObjectStore::new(&crate::config::S3Config {
      endpoint: "http://localhost:9000".to_string(),
      region: "us-east-1".to_string(),
      access_key_id: "test".to_string(),
      secret_access_key: "testsecret".to_string(),
      bucket: "test-bucket".to_string(),
    })
  }

  #[tokio::test]
  async fn s3_presigned_url_shape() {
    let store = test_store();
    let key = "avatars/some-id/image.jpg";
    let url = store.presign_get(key).await.unwrap();
    assert!(
      url.starts_with(
        "http://localhost:9000/test-bucket/avatars/some-id/image.jpg?"
      ),
      "unexpected presigned URL prefix: {url}"
    );
    for param in [
      "X-Amz-Algorithm",
      "X-Amz-Credential",
      "X-Amz-Date",
      "X-Amz-Expires",
      "X-Amz-SignedHeaders",
      "X-Amz-Signature",
    ] {
      assert!(url.contains(param), "missing {param} in {url}");
    }
    assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
    assert!(url.contains("X-Amz-Expires=86400"));
    assert!(url.contains("X-Amz-SignedHeaders=host"));
    // Credential scope slashes must be query-encoded.
    assert!(
      url.contains("X-Amz-Credential=test%2F"),
      "credential not query-encoded: {url}"
    );
  }

  #[tokio::test]
  async fn s3_authorization_header_shape() {
    let store = test_store();
    let auth = store
      .authorization(
        "GET",
        "/test-bucket/avatars/a.jpg",
        "",
        &sha256_hex(&[]),
        "20240101T000000Z",
        "20240101",
      )
      .unwrap();
    assert!(
      auth.starts_with(
        "AWS4-HMAC-SHA256 Credential=test/20240101/us-east-1/s3/aws4_request, \
         SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="
      ),
      "unexpected auth header: {auth}"
    );
    let signature = auth.rsplit('=').next().unwrap();
    assert_eq!(signature.len(), 64);
    assert!(
      signature
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
      "signature must be lowercase hex: {auth}"
    );
  }

  #[tokio::test]
  #[ignore]
  async fn live_s3_roundtrip() {
    let endpoint = std::env::var("S3_ENDPOINT").ok();
    let region = std::env::var("S3_REGION").ok();
    let access_key_id = std::env::var("S3_ACCESS_KEY_ID").ok();
    let secret_access_key = std::env::var("S3_SECRET_ACCESS_KEY").ok();
    let bucket = std::env::var("S3_BUCKET").ok();
    let (Some(endpoint), Some(region), Some(access_key_id), Some(secret), Some(bucket)) =
      (endpoint, region, access_key_id, secret_access_key, bucket)
    else {
      return;
    };
    let config = crate::config::S3Config {
      endpoint,
      region,
      access_key_id,
      secret_access_key: secret,
      bucket,
    };
    let store = ObjectStore::s3(&config);
    store.ensure_bucket().await.unwrap();
    let key =
      format!("avatars/_smoke/{}/test.bin", uuid::Uuid::now_v7());
    store
      .put(&key, b"smoke".to_vec(), "application/octet-stream")
      .await
      .unwrap();
    let url = store.presign_get(&key).await.unwrap();
    assert!(url.contains(&key), "presigned URL missing key: {url}");
    store.delete(&key).await.unwrap();
    // Second delete hits a missing key and must still succeed (404 → Ok).
    store.delete(&key).await.unwrap();
  }
}
