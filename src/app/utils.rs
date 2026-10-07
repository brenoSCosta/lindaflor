//! Shared image-upload pipeline.
//!
//! Single home for the helpers previously duplicated between
//! [`crate::auth::avatar`] and [`crate::app::admin::produtos`]:
//! magic-byte image validation, storage-key handling, object-store access,
//! and key-or-URL resolution. Uploaded bytes always land in the configured
//! [`ObjectStore`] (S3 or memory) — never in the repo tree.

use topcoat::{
  Error, Result,
  context::{Cx, try_app_context},
};

use crate::storage::{ObjectStore, StorageError};

pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;

pub const MSG_TYPE: &str = "Apenas imagens JPG, PNG ou WebP são permitidas";
pub const MSG_SIZE: &str = "A imagem deve ter no máximo 2MB";
pub const MSG_UNAVAILABLE: &str = "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde.";

const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug)]
pub struct ImageKind {
  pub extension: &'static str,
  pub content_type: &'static str,
}

/// The registered object store, or [`ObjectStore::unavailable`] when none was set.
pub fn object_store(cx: &Cx) -> ObjectStore {
  try_app_context::<ObjectStore>(cx)
    .cloned()
    .unwrap_or_else(ObjectStore::unavailable)
}

/// Validate a single upload without storing it: declared content type must
/// match the magic bytes, and the payload must fit [`MAX_IMAGE_BYTES`].
/// (Empty payloads fail classification, so they report [`MSG_TYPE`].)
pub fn validate_image(bytes: &[u8], content_type: &str) -> Result<ImageKind> {
  let kind =
    classify(content_type, bytes).ok_or_else(|| Error::msg(MSG_TYPE))?;
  if bytes.len() > MAX_IMAGE_BYTES {
    return Err(Error::msg(MSG_SIZE));
  }
  Ok(kind)
}

/// Whether `value` is an [`ObjectStore`] key as opposed to an external URL.
pub fn is_storage_key(value: &str) -> bool {
  !value.is_empty() && !value.contains("://")
}

/// Turn a stored key-or-URL into a browser-loadable URL.
///
/// External URLs pass through; storage keys are presigned. `None` (and a
/// failed presign) stays `None` so pages can fall back to a placeholder.
pub async fn resolve_storage_url(
  store: &ObjectStore,
  value: Option<&str>,
) -> Option<String> {
  let value = value.filter(|item| !item.is_empty())?;
  if value.contains("://") {
    return Some(value.to_owned());
  }
  store.presign_get(value).await.ok()
}

pub fn classify(content_type: &str, bytes: &[u8]) -> Option<ImageKind> {
  let declared = content_type.split(';').next().unwrap_or("").trim();
  if declared.eq_ignore_ascii_case("image/jpeg") && is_jpeg(bytes) {
    Some(ImageKind {
      extension: "jpg",
      content_type: "image/jpeg",
    })
  } else if declared.eq_ignore_ascii_case("image/png") && is_png(bytes) {
    Some(ImageKind {
      extension: "png",
      content_type: "image/png",
    })
  } else if declared.eq_ignore_ascii_case("image/webp") && is_webp(bytes) {
    Some(ImageKind {
      extension: "webp",
      content_type: "image/webp",
    })
  } else {
    None
  }
}

fn is_jpeg(bytes: &[u8]) -> bool {
  bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF
}

fn is_png(bytes: &[u8]) -> bool {
  bytes.starts_with(PNG_MAGIC)
}

fn is_webp(bytes: &[u8]) -> bool {
  bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
}

pub fn storage_failure(err: StorageError) -> Error {
  match err {
    StorageError::Unavailable => Error::msg(MSG_UNAVAILABLE),
    other => Error::from(other),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  pub(crate) fn png_bytes() -> Vec<u8> {
    let mut bytes = PNG_MAGIC.to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes
  }

  pub(crate) fn webp_bytes() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes
  }

  #[test]
  fn accepts_jpeg_png_webp() {
    assert!(
      classify("image/jpeg", &[0xFF, 0xD8, 0xFF, 0xD9])
        .is_some_and(|k| k.extension == "jpg")
    );
    assert!(classify("image/png", &png_bytes()).is_some());
    assert!(classify("image/webp", &webp_bytes()).is_some());
  }

  #[test]
  fn rejects_mismatched_empty_and_oversize() {
    assert!(classify("image/png", &[0xFF, 0xD8, 0xFF]).is_none());
    assert!(classify("image/gif", b"GIF89a").is_none());
    assert!(validate_image(&[], "image/png").is_err());
    assert!(validate_image(b"GIF89a", "image/gif").is_err());
    let mut bytes = PNG_MAGIC.to_vec();
    bytes.resize(MAX_IMAGE_BYTES + 1, 0);
    assert_eq!(
      validate_image(&bytes, "image/png").unwrap_err().to_string(),
      MSG_SIZE
    );
  }

  #[tokio::test]
  async fn resolve_passthrough_and_memory() {
    let store = ObjectStore::memory();
    assert_eq!(
      resolve_storage_url(&store, Some("https://picsum.photos/seed/x/100"))
        .await
        .as_deref(),
      Some("https://picsum.photos/seed/x/100")
    );
    assert_eq!(resolve_storage_url(&store, Some("")).await, None);
    store
      .put("products/p/a.jpg", b"data".to_vec(), "image/jpeg")
      .await
      .unwrap();
    assert_eq!(
      resolve_storage_url(&store, Some("products/p/a.jpg"))
        .await
        .as_deref(),
      Some("memory://products/p/a.jpg")
    );
    assert_eq!(
      resolve_storage_url(&store, Some("products/missing.jpg")).await,
      None
    );
  }
}
