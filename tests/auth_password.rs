use http::StatusCode;
use test_support::{
  json_post, pool_and_router, sign_in, sign_up, unique_email,
  verification_token_like,
};

const STRONG_PASSWORD: &str = "Harbor-Violet-Tiger-4821-Lantern!";
const STRONG_NEW_PASSWORD: &str = "Quilt-Mango-Brave-Pixel-7395?";

#[tokio::test]
async fn request_and_reset_password() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("reset");
  let old_password = STRONG_PASSWORD;
  let new_password = STRONG_NEW_PASSWORD;

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

  let token =
    verification_token_like(&pool, &format!("reset-password:{email}")).await;

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

/// The reset request queues a pt-BR mail in `email_outbox` whose link
/// carries a working one-time token (the raw token exists only there).
#[tokio::test]
async fn request_password_reset_queues_mail_with_working_link() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("reset-mail");

  let signed_up = sign_up(&router, "Mail User", &email, STRONG_PASSWORD).await;
  assert_eq!(signed_up.status, StatusCode::OK);

  let req = json_post(
    &router,
    "/api/auth/request-password-reset",
    None,
    serde_json::json!({ "email": email }),
  )
  .await;
  assert_eq!(req.status, StatusCode::OK);

  let (subject, html, text): (String, String, String) = sqlx::query_as(
    "SELECT subject, html_body, text_body FROM email_outbox
     WHERE to_address = $1 AND kind = 'password_reset' AND status = 'pending'
     ORDER BY created_at DESC LIMIT 1",
  )
  .bind(&email)
  .fetch_one(&pool)
  .await
  .expect("queued reset mail");
  assert!(subject.contains("Redefinição de senha"), "{subject}");
  assert!(text.contains("Olá, Mail User!"));
  assert!(html.contains("/reset-password?token="));

  let token = text
    .split("/reset-password?token=")
    .nth(1)
    .and_then(|rest| rest.split_whitespace().next())
    .expect("token in link")
    .to_string();

  // Log transport "delivers" the queued row without touching the network.
  let queued = lindaflor::mail::Email {
    subject,
    html,
    text,
  };
  lindaflor::mail::Mailer::Log
    .send(&email, &queued)
    .await
    .expect("log transport");

  let reset = json_post(
    &router,
    "/api/auth/reset-password",
    None,
    serde_json::json!({ "token": token, "newPassword": STRONG_NEW_PASSWORD }),
  )
  .await;
  assert_eq!(reset.status, StatusCode::OK, "{:?}", reset.json);
  let ok = sign_in(&router, &email, STRONG_NEW_PASSWORD).await;
  assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.json);
}

#[tokio::test]
async fn sign_up_rejects_weak_password() {
  let (_pool, router) = pool_and_router().await;
  let res =
    sign_up(&router, "Weak User", &unique_email("weak"), "password123").await;
  assert_eq!(res.status, StatusCode::BAD_REQUEST, "{:?}", res.json);
}

#[tokio::test]
async fn sign_up_rejects_short_password() {
  let (_pool, router) = pool_and_router().await;
  let res =
    sign_up(&router, "Short User", &unique_email("short"), "Ab1!").await;
  assert_eq!(res.status, StatusCode::BAD_REQUEST, "{:?}", res.json);
}

#[tokio::test]
async fn sign_up_rejects_password_over_max_chars() {
  let (_pool, router) = pool_and_router().await;
  // 129 chars (multibyte `é` also exercises the chars-not-bytes bound).
  let long: String = "é".repeat(129);
  assert_eq!(long.chars().count(), 129);
  let res = sign_up(&router, "Long User", &unique_email("long"), &long).await;
  assert_eq!(res.status, StatusCode::BAD_REQUEST, "{:?}", res.json);
}

#[tokio::test]
async fn reset_password_rejects_weak_new_password() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("reset-weak");
  let signed_up = sign_up(&router, "Reset Weak", &email, STRONG_PASSWORD).await;
  assert_eq!(signed_up.status, StatusCode::OK);

  let req = json_post(
    &router,
    "/api/auth/request-password-reset",
    None,
    serde_json::json!({ "email": email }),
  )
  .await;
  assert_eq!(req.status, StatusCode::OK);

  let token =
    verification_token_like(&pool, &format!("reset-password:{email}")).await;
  let reset = json_post(
    &router,
    "/api/auth/reset-password",
    None,
    serde_json::json!({
        "token": token,
        "newPassword": "password123",
    }),
  )
  .await;
  assert_eq!(reset.status, StatusCode::BAD_REQUEST, "{:?}", reset.json);
}
