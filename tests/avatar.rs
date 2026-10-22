use lindaflor::app::utils::{
  is_own_avatar_key, resolve_avatar_url, resolve_storage_url,
};
use lindaflor::auth::avatar::{remove_avatar, update_avatar};
use lindaflor::storage::ObjectStore;
use sqlx::PgPool;
use test_support::{pool_and_router, unique_email};
use uuid::Uuid;

const MSG_TYPE: &str = "Apenas imagens JPG, PNG ou WebP são permitidas";
const MSG_SIZE: &str = "A imagem deve ter no máximo 2MB";
const MSG_UNAVAILABLE: &str = "Serviço de arquivos temporariamente indisponível. Tente novamente mais tarde.";

fn png_bytes() -> Vec<u8> {
  let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
  bytes.extend_from_slice(&[0, 0, 0, 0]);
  bytes
}

fn jpeg_bytes() -> Vec<u8> {
  vec![0xFF, 0xD8, 0xFF, 0xD9]
}

fn webp_bytes() -> Vec<u8> {
  let mut bytes = b"RIFF".to_vec();
  bytes.extend_from_slice(&[0, 0, 0, 0]);
  bytes.extend_from_slice(b"WEBP");
  bytes.extend_from_slice(&[0, 0, 0, 0]);
  bytes
}

async fn insert_user(pool: &PgPool) -> Uuid {
  let id = Uuid::now_v7();
  sqlx::query(
        "INSERT INTO users (id, name, email, email_verified, created_at, updated_at)
         VALUES ($1, $2, $3, true, now(), now())",
    )
    .bind(id)
    .bind("Avatar User")
    .bind(unique_email("avatar"))
    .execute(pool)
    .await
    .expect("insert user");
  id
}

async fn user_image(pool: &PgPool, id: Uuid) -> Option<String> {
  sqlx::query_scalar::<_, Option<String>>(
    "SELECT image FROM users WHERE id = $1",
  )
  .bind(id)
  .fetch_one(pool)
  .await
  .expect("load image")
}

async fn set_image(pool: &PgPool, id: Uuid, image: &str) {
  sqlx::query("UPDATE users SET image = $2 WHERE id = $1")
    .bind(id)
    .bind(image)
    .execute(pool)
    .await
    .expect("set image");
}

#[tokio::test]
async fn accepts_png_jpeg_and_webp() {
  let (pool, _router) = pool_and_router().await;
  let store = ObjectStore::memory();
  let samples = [
    (png_bytes(), "image/png", "png"),
    (jpeg_bytes(), "image/jpeg", "jpg"),
    (webp_bytes(), "image/webp", "webp"),
  ];

  for (bytes, content_type, extension) in samples {
    let user_id = insert_user(&pool).await;
    let key = update_avatar(&pool, &store, user_id, &bytes, content_type)
      .await
      .expect(content_type);
    assert!(key.starts_with(&format!("avatars/{user_id}/")), "{key}");
    assert!(key.ends_with(&format!(".{extension}")), "{key}");
    assert_eq!(
      user_image(&pool, user_id).await.as_deref(),
      Some(key.as_str())
    );
    let url = resolve_storage_url(&store, Some(&key)).await;
    assert_eq!(url.as_deref(), Some(format!("memory://{key}").as_str()));
  }
}

#[tokio::test]
async fn rejects_gif_random_and_oversize() {
  let (pool, _router) = pool_and_router().await;
  let store = ObjectStore::memory();
  let user_id = insert_user(&pool).await;

  let gif = b"GIF89a\0\0\0\0".to_vec();
  let err = update_avatar(&pool, &store, user_id, &gif, "image/gif")
    .await
    .expect_err("gif");
  assert_eq!(err.to_string(), MSG_TYPE);

  let err =
    update_avatar(&pool, &store, user_id, &[1, 2, 3, 4, 5], "image/jpeg")
      .await
      .expect_err("random");
  assert_eq!(err.to_string(), MSG_TYPE);

  let mut oversized = png_bytes();
  oversized.resize(2 * 1024 * 1024 + 1, 0);
  let err = update_avatar(&pool, &store, user_id, &oversized, "image/png")
    .await
    .expect_err("oversize");
  assert_eq!(err.to_string(), MSG_SIZE);
  assert!(user_image(&pool, user_id).await.is_none());
}

#[tokio::test]
async fn replace_removes_previous_object() {
  let (pool, _router) = pool_and_router().await;
  let store = ObjectStore::memory();
  let user_id = insert_user(&pool).await;

  let first = update_avatar(&pool, &store, user_id, &png_bytes(), "image/png")
    .await
    .expect("first");
  let second =
    update_avatar(&pool, &store, user_id, &jpeg_bytes(), "image/jpeg")
      .await
      .expect("second");
  assert_ne!(first, second);
  assert!(store.presign_get(&first).await.is_err());
  assert_eq!(
    resolve_storage_url(&store, Some(&second)).await.as_deref(),
    Some(format!("memory://{second}").as_str())
  );
  assert_eq!(
    user_image(&pool, user_id).await.as_deref(),
    Some(second.as_str())
  );
}

#[tokio::test]
async fn external_url_is_kept_and_not_stored() {
  let (pool, _router) = pool_and_router().await;
  let store = ObjectStore::memory();
  let user_id = insert_user(&pool).await;
  let external = "https://example.com/a.png";
  set_image(&pool, user_id, external).await;

  assert_eq!(
    resolve_storage_url(&store, Some(external)).await.as_deref(),
    Some(external)
  );

  let key = update_avatar(&pool, &store, user_id, &png_bytes(), "image/png")
    .await
    .expect("upload");
  assert_eq!(
    user_image(&pool, user_id).await.as_deref(),
    Some(key.as_str())
  );
  assert!(store.presign_get(external).await.is_err());
  assert!(resolve_storage_url(&store, Some(&key)).await.is_some());
}

#[tokio::test]
async fn remove_clears_image_and_deletes_object() {
  let (pool, _router) = pool_and_router().await;
  let store = ObjectStore::memory();
  let user_id = insert_user(&pool).await;
  let key = update_avatar(&pool, &store, user_id, &png_bytes(), "image/png")
    .await
    .expect("upload");

  remove_avatar(&pool, &store, user_id).await.expect("remove");
  assert!(user_image(&pool, user_id).await.is_none());
  assert!(store.presign_get(&key).await.is_err());
}

#[tokio::test]
async fn remove_external_url_with_unavailable_store() {
  let (pool, _router) = pool_and_router().await;
  let user_id = insert_user(&pool).await;
  set_image(&pool, user_id, "https://example.com/a.png").await;

  remove_avatar(&pool, &ObjectStore::unavailable(), user_id)
    .await
    .expect("null external url");
  assert!(user_image(&pool, user_id).await.is_none());
}

#[tokio::test]
async fn unavailable_store_rejects_upload_and_keeps_storage_key() {
  let (pool, _router) = pool_and_router().await;
  let store = ObjectStore::memory();
  let user_id = insert_user(&pool).await;

  let err = update_avatar(
    &pool,
    &ObjectStore::unavailable(),
    user_id,
    &png_bytes(),
    "image/png",
  )
  .await
  .expect_err("unavailable upload");
  assert_eq!(err.to_string(), MSG_UNAVAILABLE);
  assert!(user_image(&pool, user_id).await.is_none());

  let key = update_avatar(&pool, &store, user_id, &png_bytes(), "image/png")
    .await
    .expect("upload");
  let err = remove_avatar(&pool, &ObjectStore::unavailable(), user_id)
    .await
    .expect_err("unavailable remove");
  assert_eq!(err.to_string(), MSG_UNAVAILABLE);
  assert_eq!(
    user_image(&pool, user_id).await.as_deref(),
    Some(key.as_str())
  );
  assert!(store.presign_get(&key).await.is_ok());
}

// --- B1: `users.image` can never reach objects outside the user's prefix ---

#[tokio::test]
async fn foreign_keys_in_image_are_never_deleted_or_presigned() {
  let (pool, _router) = pool_and_router().await;
  let store = ObjectStore::memory();
  let user_id = insert_user(&pool).await;

  // A storefront object whose key is public (it appears in image URLs).
  let product_key =
    format!("products/{}/{}.jpg", Uuid::now_v7(), Uuid::now_v7());
  store
    .put(&product_key, jpeg_bytes(), "image/jpeg")
    .await
    .expect("seed product image");
  set_image(&pool, user_id, &product_key).await;
  assert!(
    resolve_avatar_url(&store, user_id, Some(&product_key))
      .await
      .is_none(),
    "a foreign key must not be presigned as an avatar"
  );

  // Replacing the avatar must not delete the forged "previous" key.
  let own = update_avatar(&pool, &store, user_id, &png_bytes(), "image/png")
    .await
    .expect("upload");
  assert!(is_own_avatar_key(user_id, &own));
  assert!(store.presign_get(&product_key).await.is_ok());

  // Removing must not delete another user's avatar either.
  let other = insert_user(&pool).await;
  let other_key =
    update_avatar(&pool, &store, other, &png_bytes(), "image/png")
      .await
      .expect("other upload");
  set_image(&pool, user_id, &other_key).await;
  remove_avatar(&pool, &store, user_id).await.expect("remove");
  assert!(user_image(&pool, user_id).await.is_none());
  assert!(store.presign_get(&other_key).await.is_ok());

  // Remove with a product key: nulled, object kept.
  set_image(&pool, user_id, &product_key).await;
  remove_avatar(&pool, &store, user_id)
    .await
    .expect("remove foreign");
  assert!(user_image(&pool, user_id).await.is_none());
  assert!(store.presign_get(&product_key).await.is_ok());
}

#[test]
fn own_avatar_key_rejects_traversal_and_other_users() {
  let user_id = Uuid::now_v7();
  assert!(is_own_avatar_key(
    user_id,
    &format!("avatars/{user_id}/a.png")
  ));
  assert!(!is_own_avatar_key(
    user_id,
    &format!("avatars/{user_id}/../../products/x.jpg")
  ));
  assert!(!is_own_avatar_key(user_id, &format!("avatars/{user_id}/")));
  assert!(!is_own_avatar_key(
    user_id,
    &format!("avatars/{}/a.png", Uuid::now_v7())
  ));
  assert!(!is_own_avatar_key(user_id, "products/p/a.jpg"));
}

#[tokio::test]
async fn google_avatar_urls_pass_through_other_urls_do_not() {
  let store = ObjectStore::memory();
  let user_id = Uuid::now_v7();
  let google = "https://lh3.googleusercontent.com/a/abc=s96-c";
  assert_eq!(
    resolve_avatar_url(&store, user_id, Some(google))
      .await
      .as_deref(),
    Some(google)
  );
  for other in [
    "https://evil.example/a.png",
    "https://googleusercontent.com.evil.example/a.png",
    "http://lh3.googleusercontent.com/a.png",
  ] {
    assert!(
      resolve_avatar_url(&store, user_id, Some(other))
        .await
        .is_none(),
      "{other}"
    );
  }
}

#[tokio::test]
async fn update_user_api_rejects_client_supplied_image() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("avatar-api");
  let signed_up = test_support::sign_up(
    &router,
    "Avatar Api",
    &email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(signed_up.status, http::StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();

  let forged = test_support::json_post(
    &router,
    "/api/auth/update-user",
    cookie.as_deref(),
    serde_json::json!({ "image": "products/p/a.jpg" }),
  )
  .await;
  assert_eq!(
    forged.status,
    http::StatusCode::BAD_REQUEST,
    "{:?}",
    forged.json
  );
  assert!(user_image(&pool, user_id).await.is_none());

  // Clearing stays allowed (foreign leftovers are nulled, not deleted).
  set_image(&pool, user_id, "products/p/a.jpg").await;
  let cleared = test_support::json_post(
    &router,
    "/api/auth/update-user",
    cookie.as_deref(),
    serde_json::json!({ "image": null }),
  )
  .await;
  assert_eq!(cleared.status, http::StatusCode::OK, "{:?}", cleared.json);
  assert!(user_image(&pool, user_id).await.is_none());
}
