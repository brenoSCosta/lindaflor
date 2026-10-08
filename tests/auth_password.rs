use http::StatusCode;
use test_support::{
  json_post, pool_and_router, sign_in, sign_up, unique_email,
  verification_token,
};

#[tokio::test]
async fn request_and_reset_password() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("reset");
  let old_password = "password123";
  let new_password = "newpassword99";

  let signed_up = sign_up(&router, "Reset User", &email, old_password).await;
  assert_eq!(signed_up.status, StatusCode::OK);

  let req = json_post(
    &router,
    "/api/auth/request-password-reset",
    None,
    serde_json::json!({ "email": email }),
  )
  .await;
  assert_eq!(req.status, StatusCode::OK);
  assert_eq!(req.json["status"], true);

  let stored = sqlx::query(
    r#"
        SELECT value, value_hash
        FROM verifications
        WHERE identifier LIKE $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
  )
  .bind(format!("reset-password:{email}%"))
  .fetch_one(&pool)
  .await
  .expect("reset token row");
  let stored_value: String = sqlx::Row::get(&stored, "value");
  let stored_hash: Option<String> = sqlx::Row::get(&stored, "value_hash");
  assert!(
    stored_value.is_empty(),
    "reset token must not be stored plaintext"
  );
  assert!(
    stored_hash
      .as_deref()
      .is_some_and(|h| h.len() == 64 && h != stored_value),
    "value_hash must be a digest, not the raw secret"
  );

  let token = verification_token(&pool, "reset-password:").await;

  let reset = json_post(
    &router,
    "/api/auth/reset-password",
    None,
    serde_json::json!({
        "token": token,
        "newPassword": new_password,
    }),
  )
  .await;
  assert_eq!(reset.status, StatusCode::OK, "{:?}", reset.json);
  assert_eq!(reset.json["status"], true);

  let old = sign_in(&router, &email, old_password).await;
  assert_eq!(old.status, StatusCode::UNAUTHORIZED);

  let ok = sign_in(&router, &email, new_password).await;
  assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.json);
}
