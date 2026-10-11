use http::StatusCode;
use test_support::{
  json_get, json_post, pool_and_router, sign_in, sign_up, unique_email,
  verification_token_like,
};

#[tokio::test]
async fn send_and_verify_email() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("verify");

  let signed_up = sign_up(
    &router,
    "Verify User",
    &email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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

  let stored = sqlx::query(
    r#"
        SELECT value, value_hash
        FROM verifications
        WHERE identifier LIKE $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
  )
  .bind(format!("email-verification:{email}%"))
  .fetch_one(&pool)
  .await
  .expect("verify token row");
  let stored_value: String = sqlx::Row::get(&stored, "value");
  assert!(
    stored_value.is_empty(),
    "verify token must not be plaintext"
  );

  let token =
    verification_token_like(&pool, &format!("email-verification:{email}"))
      .await;

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

  let signed_up = sign_up(
    &router,
    "Change Email",
    &email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.as_deref();

  let change = json_post(
    &router,
    "/api/auth/change-email",
    cookie,
    serde_json::json!({
        "newEmail": new_email,
        "password": "Harbor-Violet-Tiger-4821-Lantern!",
    }),
  )
  .await;
  assert_eq!(change.status, StatusCode::OK, "{:?}", change.json);
  assert_eq!(change.json["status"], true);

  let token =
    verification_token_like(&pool, &format!("change-email:%:{new_email}"))
      .await;

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

/// Requesting an email change is a credential change: every other session
/// is revoked immediately while the authorizing session keeps working.
#[tokio::test]
async fn change_email_request_revokes_other_sessions() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("change-req-from");
  let new_email = unique_email("change-req-to");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Change Req", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie_a = signed_up.next_cookie.clone().expect("cookie a");
  let signed_in = sign_in(&router, &email, password).await;
  assert_eq!(signed_in.status, StatusCode::OK);
  let cookie_b = signed_in.next_cookie.clone().expect("cookie b");

  let change = json_post(
    &router,
    "/api/auth/change-email",
    Some(&cookie_b),
    serde_json::json!({
        "newEmail": new_email,
        "password": password,
    }),
  )
  .await;
  assert_eq!(change.status, StatusCode::OK, "{:?}", change.json);

  let stale = json_get(&router, "/api/auth/get-session", Some(&cookie_a)).await;
  assert!(
    stale.json.is_null(),
    "other session must be revoked on change-email request, got {:?}",
    stale.json
  );
  let current =
    json_get(&router, "/api/auth/get-session", Some(&cookie_b)).await;
  assert_eq!(current.json["user"]["email"], email);
}

/// Confirming the new address revokes every session (including the
/// requester's) while the token-only confirm keeps working logged-out.
#[tokio::test]
async fn change_email_confirm_revokes_all_sessions_logged_out() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("change-confirm-from");
  let new_email = unique_email("change-confirm-to");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Change Confirm", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie_a = signed_up.next_cookie.clone().expect("cookie a");
  let signed_in = sign_in(&router, &email, password).await;
  assert_eq!(signed_in.status, StatusCode::OK);
  let cookie_b = signed_in.next_cookie.clone().expect("cookie b");

  let change = json_post(
    &router,
    "/api/auth/change-email",
    Some(&cookie_b),
    serde_json::json!({
        "newEmail": new_email,
        "password": password,
    }),
  )
  .await;
  assert_eq!(change.status, StatusCode::OK, "{:?}", change.json);

  let token =
    verification_token_like(&pool, &format!("change-email:%:{new_email}"))
      .await;

  // Token-only: no session cookie, exactly like a logged-out mail link.
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

  for (name, cookie) in [("requester", &cookie_b), ("other", &cookie_a)] {
    let revoked =
      json_get(&router, "/api/auth/get-session", Some(cookie)).await;
    assert!(
      revoked.json.is_null(),
      "{name} session must be revoked on confirm, got {:?}",
      revoked.json
    );
  }
}
