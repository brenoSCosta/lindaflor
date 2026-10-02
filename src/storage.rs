use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::primitives::ByteStream;

const PRESIGN_EXPIRY_SECS: u64 = 86_400;

#[derive(Clone)]
pub struct S3ObjectStore {
  client: aws_sdk_s3::Client,
  bucket: String,
  region: String,
}

impl S3ObjectStore {
  fn new(config: &crate::config::S3Config) -> Self {
    let creds = aws_sdk_s3::config::Credentials::new(
      config.access_key_id.clone(),
      config.secret_access_key.clone(),
      None,
      None,
      "lindaflor",
    );
    let conf = aws_sdk_s3::config::Builder::new()
      .behavior_version_latest()
      .region(aws_sdk_s3::config::Region::new(config.region.clone()))
      .endpoint_url(&config.endpoint)
      .credentials_provider(creds)
      .force_path_style(true)
      .request_checksum_calculation(
        aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired,
      )
      .response_checksum_validation(
        aws_sdk_s3::config::ResponseChecksumValidation::WhenRequired,
      )
      .build();
    Self {
      client: aws_sdk_s3::Client::from_conf(conf),
      bucket: config.bucket.clone(),
      region: config.region.clone(),
    }
  }

  async fn put(
    &self,
    key: &str,
    body: Vec<u8>,
    content_type: &str,
  ) -> Result<(), StorageError> {
    self
      .client
      .put_object()
      .bucket(&self.bucket)
      .key(key)
      .body(ByteStream::from(body))
      .content_type(content_type)
      .send()
      .await
      .map_err(StorageError::from_aws)?;
    Ok(())
  }

  async fn delete(&self, key: &str) -> Result<(), StorageError> {
    self
      .client
      .delete_object()
      .bucket(&self.bucket)
      .key(key)
      .send()
      .await
      .map_err(StorageError::from_aws)?;
    Ok(())
  }

  async fn presign_get(&self, key: &str) -> Result<String, StorageError> {
    let presigning =
      PresigningConfig::expires_in(Duration::from_secs(PRESIGN_EXPIRY_SECS))
        .map_err(StorageError::from_aws)?;
    let presigned = self
      .client
      .get_object()
      .bucket(&self.bucket)
      .key(key)
      .presigned(presigning)
      .await
      .map_err(StorageError::from_aws)?;
    Ok(presigned.uri().to_string())
  }

  async fn ensure_bucket(&self) -> Result<(), StorageError> {
    let Err(err) = self.client.head_bucket().bucket(&self.bucket).send().await
    else {
      return Ok(());
    };
    let service_missing = err.as_service_error().is_some_and(|service| {
      service.is_not_found()
        || matches!(service.meta().code(), Some("NotFound" | "NoSuchBucket"))
    });
    let status_missing = err
      .raw_response()
      .is_some_and(|response| response.status().as_u16() == 404);
    if service_missing || status_missing {
      self.create_bucket().await
    } else {
      Err(StorageError::from_aws(err))
    }
  }

  async fn create_bucket(&self) -> Result<(), StorageError> {
    let mut req = self.client.create_bucket().bucket(&self.bucket);
    // us-east-1 rejects a location constraint. Other regions require one.
    if !self.region.eq_ignore_ascii_case("us-east-1") {
      let location =
        aws_sdk_s3::types::BucketLocationConstraint::from(self.region.as_str());
      let cfg = aws_sdk_s3::types::CreateBucketConfiguration::builder()
        .location_constraint(location)
        .build();
      req = req.create_bucket_configuration(cfg);
    }
    req.send().await.map_err(StorageError::from_aws)?;
    Ok(())
  }
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
}
