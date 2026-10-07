use sqlx::PgPool;
use topcoat::{Error, Result};
use uuid::Uuid;

use crate::app::utils::{
  MSG_UNAVAILABLE, is_storage_key, storage_failure, validate_image,
};
use crate::storage::{ObjectStore, StorageError};

const MSG_UPDATE: &str = "Falha ao atualizar avatar";

/// Validate `bytes`, store them, and save the object key on the user.
pub async fn update_avatar(
  pool: &PgPool,
  store: &ObjectStore,
  user_id: Uuid,
  bytes: &[u8],
  content_type: &str,
) -> Result<String> {
  let kind = validate_image(bytes, content_type)?;
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

async fn current_image(pool: &PgPool, user_id: Uuid) -> Result<Option<String>> {
  let image = sqlx::query_scalar::<_, Option<String>>(
    "SELECT image FROM users WHERE id = $1",
  )
  .bind(user_id)
  .fetch_optional(pool)
  .await?;
  Ok(image.flatten())
}
