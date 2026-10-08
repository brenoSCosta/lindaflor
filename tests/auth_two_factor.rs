use http::StatusCode;
use test_support::{
  json_post, pool_and_router, sign_in, sign_up, totp_code, unique_email,
};

#[tokio::test]
async fn enable_verify_totp_then_sign_in_requires_2fa() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("2fa");
  let password = "password123";

  let signed_up = sign_up(&router, "Two Factor", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();

  let enable = json_post(
    &router,
    "/api/auth/two-factor/enable",
    cookie.as_deref(),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);
  let secret = enable.json["secret"].as_str().expect("secret").to_string();
  assert!(!secret.is_empty());
  assert!(
    enable.json["totpUri"]
      .as_str()
      .unwrap()
      .contains("otpauth://")
  );
  let backup = enable.json["backupCodes"][0]
    .as_str()
    .expect("backup code")
    .to_string();

  let code = totp_code(&secret);
  let verify_enable = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    cookie.as_deref(),
    serde_json::json!({ "code": code }),
  )
  .await;
  assert_eq!(
    verify_enable.status,
    StatusCode::OK,
    "{:?}",
    verify_enable.json
  );
  assert_eq!(verify_enable.json["user"]["twoFactorEnabled"], true);

  // Sign out so the next sign-in must pass 2FA.
  let _ = json_post(
    &router,
    "/api/auth/sign-out",
    cookie.as_deref(),
    serde_json::json!({}),
  )
  .await;

  let challenged = sign_in(&router, &email, password).await;
  assert_eq!(
    challenged.status,
    StatusCode::FORBIDDEN,
    "{:?}",
    challenged.json
  );
  assert_eq!(challenged.json["code"], "TWO_FACTOR_REQUIRED");
  let token = challenged.json["token"]
    .as_str()
    .expect("2fa token")
    .to_string();

  // Enable already reserved this step's TOTP; complete sign-in with a backup
  // code so this test stays independent of the 90s replay window.
  let completed = json_post(
    &router,
    "/api/auth/two-factor/verify-backup-code",
    None,
    serde_json::json!({
        "code": backup,
        "token": token,
    }),
  )
  .await;
  assert_eq!(completed.status, StatusCode::OK, "{:?}", completed.json);
  assert!(completed.cookie.is_some());
  assert_eq!(completed.json["user"]["email"], email);
  assert_eq!(completed.json["user"]["twoFactorEnabled"], true);
}

#[tokio::test]
async fn totp_code_cannot_be_replayed() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("2fa-replay");
  let password = "password123";

  let signed_up = sign_up(&router, "Replay", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();

  let enable = json_post(
    &router,
    "/api/auth/two-factor/enable",
    cookie.as_deref(),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);
  let secret = enable.json["secret"].as_str().expect("secret").to_string();
  let backup = enable.json["backupCodes"][0]
    .as_str()
    .expect("backup code")
    .to_string();

  let code = totp_code(&secret);
  let verify_enable = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    cookie.as_deref(),
    serde_json::json!({ "code": code }),
  )
  .await;
  assert_eq!(
    verify_enable.status,
    StatusCode::OK,
    "{:?}",
    verify_enable.json
  );

  let replay_enable = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    cookie.as_deref(),
    serde_json::json!({ "code": code }),
  )
  .await;
  assert_ne!(
    replay_enable.status,
    StatusCode::OK,
    "reused TOTP must not confirm enable twice: {:?}",
    replay_enable.json
  );

  let _ = json_post(
    &router,
    "/api/auth/sign-out",
    cookie.as_deref(),
    serde_json::json!({}),
  )
  .await;

  let challenged = sign_in(&router, &email, password).await;
  assert_eq!(challenged.status, StatusCode::FORBIDDEN);
  let token = challenged.json["token"]
    .as_str()
    .expect("2fa token")
    .to_string();

  let replay_login = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    None,
    serde_json::json!({
        "code": code,
        "token": token,
    }),
  )
  .await;
  assert_ne!(
    replay_login.status,
    StatusCode::OK,
    "same TOTP must not complete sign-in after enable: {:?}",
    replay_login.json
  );

  let completed = json_post(
    &router,
    "/api/auth/two-factor/verify-backup-code",
    None,
    serde_json::json!({
        "code": backup,
        "token": token,
    }),
  )
  .await;
  assert_eq!(completed.status, StatusCode::OK, "{:?}", completed.json);
}
