mod common;

use common::{
  json_post, pool_and_router, sign_up, unique_email, verification_token,
};
use http::StatusCode;

#[tokio::test]
async fn send_and_verify_email() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("verify");

  let signed_up = sign_up(&router, "Verify User", &email, "password123").await;
  assert_eq!(signed_up.status, StatusCode::OK);
  assert_eq!(signed_up.json["user"]["emailVerified"], false);
  let cookie = signed_up.next_cookie.as_deref();

  let send = json_post(
    &router,
    "/api/auth/send-verification-email",
    cookie,
    serde_json::json!({ "email": email }),
  )
  .await;
  assert_eq!(send.status, StatusCode::OK);
  assert_eq!(send.json["status"], true);

  let token = verification_token(&pool, "email-verification:").await;

  let verify = json_post(
    &router,
    "/api/auth/verify-email",
    None,
    serde_json::json!({ "token": token }),
  )
  .await;
  assert_eq!(verify.status, StatusCode::OK, "{:?}", verify.json);
  assert_eq!(verify.json["status"], true);

  let verified: bool = sqlx::query_scalar!(
    r#"SELECT email_verified AS "email_verified!" FROM users WHERE email = $1"#,
    email
  )
  .fetch_one(&pool)
  .await
  .unwrap();
  assert!(verified);
}

#[tokio::test]
async fn change_email_and_confirm() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("change-from");
  let new_email = unique_email("change-to");

  let signed_up = sign_up(&router, "Change Email", &email, "password123").await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.as_deref();

  let change = json_post(
    &router,
    "/api/auth/change-email",
    cookie,
    serde_json::json!({ "newEmail": new_email }),
  )
  .await;
  assert_eq!(change.status, StatusCode::OK, "{:?}", change.json);
  assert_eq!(change.json["status"], true);

  let token = verification_token(&pool, "change-email:").await;

  let confirm = json_post(
    &router,
    "/api/auth/change-email/confirm",
    None,
    serde_json::json!({ "token": token }),
  )
  .await;
  assert_eq!(confirm.status, StatusCode::OK, "{:?}", confirm.json);
  assert_eq!(confirm.json["status"], true);

  let stored: String = sqlx::query_scalar!(
    r#"SELECT email AS "email!" FROM users WHERE email = $1"#,
    new_email
  )
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(stored, new_email);
}
