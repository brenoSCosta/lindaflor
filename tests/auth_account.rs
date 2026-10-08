use http::StatusCode;
use test_support::{
  json_get, json_post, pool_and_router, sign_in, sign_up, unique_email,
  verification_token,
};
use uuid::Uuid;

#[tokio::test]
async fn update_user_change_password_list_accounts() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("account");
  let password = "password123";

  let signed_up = sign_up(&router, "Account User", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();

  let updated = json_post(
    &router,
    "/api/auth/update-user",
    cookie.as_deref(),
    serde_json::json!({ "name": "Updated Name" }),
  )
  .await;
  assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
  assert_eq!(updated.json["user"]["name"], "Updated Name");

  let changed = json_post(
    &router,
    "/api/auth/change-password",
    cookie.as_deref(),
    serde_json::json!({
        "currentPassword": password,
        "newPassword": "password456",
    }),
  )
  .await;
  assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.json);

  let accounts =
    json_get(&router, "/api/auth/list-accounts", cookie.as_deref()).await;
  assert_eq!(accounts.status, StatusCode::OK);
  let list = accounts.json.as_array().expect("accounts array");
  assert_eq!(list.len(), 1);
  assert_eq!(list[0]["providerId"], "credential");

  // Insert a second login method so unlink of credential is allowed, then unlink the fake one.
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();
  let google_account_id = Uuid::now_v7();
  sqlx::query!(
        r#"
        INSERT INTO accounts (id, account_id, provider_id, user_id, created_at, updated_at)
        VALUES ($1, $2, 'google', $3, now(), now())
        "#,
        google_account_id,
        format!("google-{user_id}"),
        user_id,
    )
    .execute(&pool)
    .await
    .unwrap();

  let unlink = json_post(
    &router,
    "/api/auth/unlink-account",
    cookie.as_deref(),
    serde_json::json!({
        "providerId": "google",
        "accountId": format!("google-{user_id}"),
    }),
  )
  .await;
  assert_eq!(unlink.status, StatusCode::OK, "{:?}", unlink.json);
  assert_eq!(unlink.json["status"], true);

  let accounts2 =
    json_get(&router, "/api/auth/list-accounts", cookie.as_deref()).await;
  assert_eq!(accounts2.json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn delete_user_with_token() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("delete");
  let password = "password123";

  let signed_up = sign_up(&router, "Delete Me", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();
  let user_id = signed_up.json["user"]["id"].as_str().unwrap().to_string();

  let request = json_post(
    &router,
    "/api/auth/delete-user",
    cookie.as_deref(),
    serde_json::json!({ "password": password }),
  )
  .await;
  assert_eq!(request.status, StatusCode::OK, "{:?}", request.json);

  let token = verification_token(&pool, "delete-user:").await;

  let deleted = json_post(
    &router,
    "/api/auth/delete-user",
    cookie.as_deref(),
    serde_json::json!({ "token": token }),
  )
  .await;
  assert_eq!(deleted.status, StatusCode::OK, "{:?}", deleted.json);

  let gone = sqlx::query_scalar!(
    r#"SELECT id AS "id!" FROM users WHERE id = $1"#,
    Uuid::parse_str(&user_id).unwrap()
  )
  .fetch_optional(&pool)
  .await
  .unwrap();
  assert!(gone.is_none());

  let refuse = sign_in(&router, &email, password).await;
  assert_eq!(refuse.status, StatusCode::UNAUTHORIZED);
}

struct EnvRestore {
  key: &'static str,
  previous: Option<String>,
}

impl Drop for EnvRestore {
  fn drop(&mut self) {
    match self.previous.take() {
      Some(value) => unsafe { std::env::set_var(self.key, value) },
      None => unsafe { std::env::remove_var(self.key) },
    }
  }
}

fn set_env(key: &'static str, value: &str) -> EnvRestore {
  let previous = std::env::var(key).ok();
  unsafe { std::env::set_var(key, value) };
  EnvRestore { key, previous }
}

fn remove_env(key: &'static str) -> EnvRestore {
  let previous = std::env::var(key).ok();
  unsafe { std::env::remove_var(key) };
  EnvRestore { key, previous }
}

/// devenv (and other tests) may set Google OAuth env; serialize mutations.
static GOOGLE_ENV_LOCK: tokio::sync::Mutex<()> =
  tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn link_social_unavailable_without_google_env() {
  let _lock = GOOGLE_ENV_LOCK.lock().await;
  let _id = remove_env("GOOGLE_CLIENT_ID");
  let _secret = remove_env("GOOGLE_CLIENT_SECRET");
  let _origin = remove_env("APP_ORIGIN");

  let (_pool, router) = pool_and_router().await;
  let email = unique_email("link");
  let signed_up = sign_up(&router, "Link User", &email, "password123").await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();

  let linked = json_post(
    &router,
    "/api/auth/link-social",
    cookie.as_deref(),
    serde_json::json!({ "provider": "google" }),
  )
  .await;
  assert_eq!(
    linked.status,
    StatusCode::NOT_IMPLEMENTED,
    "{:?}",
    linked.json
  );
}

#[tokio::test]
async fn google_callback_rejects_missing_pkce_verifier_before_network() {
  let _lock = GOOGLE_ENV_LOCK.lock().await;
  let _id =
    set_env("GOOGLE_CLIENT_ID", "test-client.apps.googleusercontent.com");
  let _secret = set_env("GOOGLE_CLIENT_SECRET", "test-secret");
  let _origin = set_env("APP_ORIGIN", "http://127.0.0.1:4200");

  let (pool, router) = pool_and_router().await;
  let state = format!("pkce-missing-{}", Uuid::now_v7().simple());
  let value = serde_json::json!({ "callback": "/dashboard" }).to_string();
  sqlx::query!(
    r#"
        INSERT INTO verifications (id, identifier, value, expires_at, created_at, updated_at)
        VALUES ($1, $2, $3, now() + interval '10 minutes', now(), now())
        "#,
    Uuid::now_v7(),
    format!("oauth-google:{state}"),
    value,
  )
  .execute(&pool)
  .await
  .expect("insert oauth state");

  let result = json_get(
    &router,
    &format!("/api/auth/callback/google?code=fake-code&state={state}"),
    None,
  )
  .await;
  assert_eq!(result.status, StatusCode::BAD_REQUEST, "{:?}", result.json);
}
