use sqlx::PgPool;
use topcoat::{
  Error, Result,
  context::{Cx, try_app_context},
};
use uuid::Uuid;

use crate::storage::{ObjectStore, StorageError};

const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

const MSG_TYPE: &str = "Apenas imagens JPG, PNG ou WebP são permitidas";
const MSG_SIZE: &str = "A imagem deve ter no máximo 2MB";
const MSG_UNAVAILABLE: &str = "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde.";
const MSG_UPDATE: &str = "Falha ao atualizar avatar";

const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

struct ImageKind {
  extension: &'static str,
  content_type: &'static str,
}

/// The registered object store, or [`ObjectStore::unavailable`] when none was set.
pub fn object_store(cx: &Cx) -> ObjectStore {
  try_app_context::<ObjectStore>(cx)
    .cloned()
    .unwrap_or_else(ObjectStore::unavailable)
}

/// Validate `bytes`, store them, and save the object key on the user.
pub async fn update_avatar(
  pool: &PgPool,
  store: &ObjectStore,
  user_id: Uuid,
  bytes: &[u8],
  content_type: &str,
) -> Result<String> {
  let kind =
    classify(content_type, bytes).ok_or_else(|| Error::msg(MSG_TYPE))?;
  if bytes.len() > MAX_AVATAR_BYTES {
    return Err(Error::msg(MSG_SIZE));
  }
  if matches!(store, ObjectStore::Unavailable) {
    return Err(Error::msg(MSG_UNAVAILABLE));
  }

  let previous = current_image(pool, user_id).await?;
  let key = format!(
    "avatars/{user_id}/{}.{ext}",
    Uuid::now_v7(),
    ext = kind.extension
  );
  store
    .put(&key, bytes.to_vec(), kind.content_type)
    .await
    .map_err(storage_failure)?;

  let updated = sqlx::query(
    "UPDATE users SET image = $2, updated_at = now() WHERE id = $1",
  )
  .bind(user_id)
  .bind(&key)
  .execute(pool)
  .await?;
  if updated.rows_affected() == 0 {
    let _ = store.delete(&key).await;
    return Err(Error::msg(MSG_UPDATE));
  }

  if let Some(previous) = previous
    && is_storage_key(&previous)
    && previous != key
  {
    let _ = store.delete(&previous).await;
  }

  Ok(key)
}

/// Clear `users.image`. Storage keys are deleted; external URLs are only nulled.
pub async fn remove_avatar(
  pool: &PgPool,
  store: &ObjectStore,
  user_id: Uuid,
) -> Result<()> {
  let current = current_image(pool, user_id).await?;
  if let Some(key) = current.filter(|image| is_storage_key(image)) {
    if matches!(store, ObjectStore::Unavailable) {
      return Err(Error::msg(MSG_UNAVAILABLE));
    }
    if let Err(StorageError::Unavailable) = store.delete(&key).await {
      return Err(Error::msg(MSG_UNAVAILABLE));
    }
  }

  sqlx::query(
    "UPDATE users SET image = NULL, updated_at = now() WHERE id = $1",
  )
  .bind(user_id)
  .execute(pool)
  .await?;
  Ok(())
}

/// Turn a stored image value into a URL the browser can load.
///
/// `None` and a failed presign stay `None` so the page can fall back to initials.
pub async fn resolve_avatar_url(
  store: &ObjectStore,
  image: Option<&str>,
) -> Option<String> {
  let image = image.filter(|value| !value.is_empty())?;
  if image.contains("://") {
    return Some(image.to_owned());
  }
  store.presign_get(image).await.ok()
}

fn classify(content_type: &str, bytes: &[u8]) -> Option<ImageKind> {
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

fn is_storage_key(image: &str) -> bool {
  !image.is_empty() && !image.contains("://")
}

fn storage_failure(err: StorageError) -> Error {
  match err {
    StorageError::Unavailable => Error::msg(MSG_UNAVAILABLE),
    other => Error::from(other),
  }
}

async fn current_image(pool: &PgPool, user_id: Uuid) -> Result<Option<String>> {
  let image = sqlx::query_scalar::<_, Option<String>>(
    "SELECT image FROM users WHERE id = $1",
  )
  .bind(user_id)
  .fetch_optional(pool)
  .await?;
  Ok(image.flatten())
}
