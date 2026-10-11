use http::StatusCode;
use test_support::{
  json_post, pool_and_router, sign_in, sign_up, totp_code, unique_email,
};
use topcoat::router::Body;
use uuid::Uuid;

// --- P2-3: TOTP secrets encrypted at rest + strict one-time display ---

/// TOTP code for the step *after* the current one. `skew=1` accepts it, and
/// the replay cache (`SET NX EX 90s` on the presented code) has never seen
/// it — so a TOTP login right after enable-verify is deterministic without
/// sleeping for a step rollover.
fn totp_code_next_step(secret_b32: &str) -> String {
  use std::time::{SystemTime, UNIX_EPOCH};
  use totp_rs::{Algorithm, Builder, Secret};

  let secret = Secret::try_from_base32(secret_b32).expect("base32 secret");
  let totp = Builder::new()
    .with_algorithm(Algorithm::SHA1)
    .with_digits(6)
    .with_skew(1)
    .with_step_duration(30)
    .with_secret(secret)
    .build()
    .expect("totp builder");
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("clock")
    .as_secs();
  totp.generate(now + 30).to_string()
}

#[tokio::test]
async fn totp_secret_is_encrypted_at_rest() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("2fa-enc");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Encrypted", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();

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

  // At rest: nonce + ciphertext present and NOT NULL; no plaintext column.
  let row = sqlx::query(
    "SELECT secret_nonce, secret_ciphertext, secret_key_version, verified
     FROM two_factor WHERE user_id = $1",
  )
  .bind(user_id)
  .fetch_one(&pool)
  .await
  .expect("two_factor row");
  use sqlx::Row;
  let nonce: Vec<u8> = row.get("secret_nonce");
  let ciphertext: Vec<u8> = row.get("secret_ciphertext");
  let version: i16 = row.get("secret_key_version");
  let verified: bool = row.get("verified");
  assert!(!verified);
  assert_eq!(nonce.len(), 24, "XChaCha20 nonce is 24 bytes");
  assert_eq!(version, 1);
  assert!(!ciphertext.is_empty());
  assert_ne!(
    ciphertext,
    secret.as_bytes(),
    "ciphertext must not embed the plaintext"
  );

  // The stored ciphertext decrypts (same-process key) to the one-time secret.
  let decrypted =
    lindaflor::auth::totp_crypto::decrypt_secret(&nonce, &ciphertext, 1)
      .expect("decrypt");
  assert_eq!(decrypted.as_str(), secret);

  // Full flow still works end to end.
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

  let _ = json_post(
    &router,
    "/api/auth/sign-out",
    cookie.as_deref(),
    serde_json::json!({}),
  )
  .await;

  // TOTP login decrypts the same row (next-step code: valid per skew=1,
  // never presented, so the replay cache cannot claim it).
  let challenged = sign_in(&router, &email, password).await;
  assert_eq!(challenged.status, StatusCode::FORBIDDEN);
  assert_eq!(challenged.json["code"], "TWO_FACTOR_REQUIRED");
  let token = challenged.json["token"]
    .as_str()
    .expect("2fa token")
    .to_string();
  let login = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    None,
    serde_json::json!({
        "code": totp_code_next_step(&secret),
        "token": token,
    }),
  )
  .await;
  assert_eq!(login.status, StatusCode::OK, "{:?}", login.json);
  assert!(login.cookie.is_some());
}

#[tokio::test]
async fn missing_or_tampered_secret_fails_hard() {
  // Direct-only: secrets are always encrypted at rest. Any decrypt
  // failure (wrong key, bad nonce, version mismatch) fails closed with
  // no plaintext fallback.
  let bad_nonce = vec![0u8; 24];
  assert!(
    lindaflor::auth::totp_crypto::decrypt_secret(&bad_nonce, b"tag", 1)
      .is_err(),
    "random nonce/ciphertext must fail hard"
  );
  assert!(
    lindaflor::auth::totp_crypto::decrypt_secret(&bad_nonce, b"tag", 999)
      .is_err(),
    "version mismatch must fail hard"
  );
  assert!(
    lindaflor::auth::totp_crypto::decrypt_secret(&[0u8; 12], b"tag", 1)
      .is_err(),
    "short nonce must fail hard"
  );
}

#[tokio::test]
async fn two_factor_secret_index_is_gone() {
  let (pool, _router) = pool_and_router().await;
  let names: Vec<String> = sqlx::query_scalar(
    "SELECT indexname FROM pg_indexes WHERE schemaname = 'public' AND tablename = 'two_factor'",
  )
  .fetch_all(&pool)
  .await
  .expect("list indexes");
  assert!(
    !names.iter().any(|n| n == "two_factor_secret_idx"),
    "plaintext secret index must be dropped, found: {names:?}"
  );
  assert!(
    names.iter().any(|n| n == "two_factor_user_id_idx"),
    "user_id index must be kept, found: {names:?}"
  );
  let cols: Vec<(String, String)> = sqlx::query_as(
    "SELECT column_name, is_nullable FROM information_schema.columns
     WHERE table_schema = 'public' AND table_name = 'two_factor'",
  )
  .fetch_all(&pool)
  .await
  .expect("list columns");
  let names: Vec<&str> = cols.iter().map(|(name, _)| name.as_str()).collect();
  assert!(
    !names.contains(&"secret"),
    "plaintext secret column must be dropped, found: {names:?}"
  );
  for col in ["secret_nonce", "secret_ciphertext", "secret_key_version"] {
    let nullable = cols
      .iter()
      .find(|(name, _)| name == col)
      .map(|(_, nullable)| nullable.as_str());
    assert_eq!(
      nullable,
      Some("NO"),
      "{col} must be NOT NULL, found: {cols:?}"
    );
  }
}

#[tokio::test]
async fn settings_setup_page_leaks_no_secret() {
  // Force-link the settings page into this test binary so `discover()`
  // mounts it (module pages are GC'd from test binaries otherwise).
  let _ = lindaflor::app::settings::page;
  let _ = lindaflor::app::settings::settings_post;
  let _ = lindaflor::app::settings::two_factor_setup;
  let _ = lindaflor::app::settings::two_factor_setup_enable;

  let (_pool, router) = pool_and_router().await;
  let email = unique_email("2fa-noleak");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "No Leak", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone().expect("cookie");

  // Start an enrollment (unverified pending row exists, secret shown once).
  let enable = json_post(
    &router,
    "/api/auth/two-factor/enable",
    Some(&cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);
  let secret = enable.json["secret"].as_str().expect("secret").to_string();
  let totp_uri = enable.json["totpUri"]
    .as_str()
    .expect("totpUri")
    .to_string();

  for path in [
    "/settings?setup=1",
    "/settings?tab=security",
    "/settings/two-factor",
  ] {
    let req = http::Request::builder()
      .method(http::Method::GET)
      .uri(path)
      .header(http::header::COOKIE, &cookie)
      .body(Body::empty())
      .expect("build request");
    let response = router.handle(req).await;
    let status = response.status();
    let body = test_support::body_json(response).await;
    let text = body["_body"].as_str().unwrap_or("");
    assert_eq!(
      status,
      StatusCode::OK,
      "GET {path} must render, got: {text}"
    );
    assert!(!text.contains(&secret), "GET {path} leaked the TOTP secret");
    assert!(
      !text.contains(&totp_uri),
      "GET {path} leaked the otpauth URI"
    );
    assert!(
      !text.contains("otpauth://"),
      "GET {path} leaked an otpauth URI"
    );
    assert!(
      !text.contains("data:image/svg+xml"),
      "GET {path} leaked a QR code"
    );
  }
}

async fn get_html(
  router: &topcoat::router::Router,
  path: &str,
  cookie: &str,
) -> (StatusCode, String) {
  let req = http::Request::builder()
    .method(http::Method::GET)
    .uri(path)
    .header(http::header::COOKIE, cookie)
    .body(Body::empty())
    .expect("build request");
  let response = router.handle(req).await;
  let status = response.status();
  let body = test_support::body_json(response).await;
  let text = body["_body"].as_str().unwrap_or("").to_string();
  (status, text)
}

#[tokio::test]
async fn html_enable_shows_secret_once_on_setup_page() {
  let _ = lindaflor::app::settings::page;
  let _ = lindaflor::app::settings::settings_post;
  let _ = lindaflor::app::settings::two_factor_setup;
  let _ = lindaflor::app::settings::two_factor_setup_enable;

  let (_pool, router) = pool_and_router().await;
  let email = unique_email("2fa-html");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Html Enable", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone().expect("cookie");

  let req = http::Request::builder()
    .method(http::Method::POST)
    .uri("/settings/two-factor")
    .header(http::header::COOKIE, &cookie)
    .header(
      http::header::CONTENT_TYPE,
      "application/x-www-form-urlencoded",
    )
    .body(Body::from("action=enable_2fa"))
    .expect("build request");
  let response = router.handle(req).await;
  let status = response.status();
  let location = response
    .headers()
    .get(http::header::LOCATION)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")
    .to_string();
  assert_eq!(
    status,
    StatusCode::SEE_OTHER,
    "HTML enable must 303 to the setup page, got {status} loc={location}"
  );
  assert!(
    location.contains("/settings/two-factor"),
    "expected setup Location, got {location}"
  );

  let (status, text) = get_html(&router, "/settings/two-factor", &cookie).await;
  assert_eq!(status, StatusCode::OK, "{text}");
  assert!(
    text.contains("data:image/svg+xml"),
    "first setup GET must show a QR code"
  );
  assert!(
    !text.contains("otpauth://"),
    "setup page must not embed the otpauth URI (it contains the secret)"
  );
  assert!(
    text.contains("Códigos de recuperação"),
    "first setup GET must show backup codes"
  );

  let (status, text) = get_html(&router, "/settings/two-factor", &cookie).await;
  assert_eq!(status, StatusCode::OK, "{text}");
  assert!(
    !text.contains("otpauth://"),
    "second setup GET must not re-display the secret"
  );
  assert!(
    !text.contains("data:image/svg+xml"),
    "second setup GET must not re-display the QR"
  );

  let (status, text) =
    get_html(&router, "/settings?tab=security", &cookie).await;
  assert_eq!(status, StatusCode::OK, "{text}");
  assert!(!text.contains("otpauth://"));
  assert!(!text.contains("data:image/svg+xml"));
}

#[tokio::test]
async fn enable_verify_totp_then_sign_in_requires_2fa() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("2fa");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

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
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

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

#[tokio::test]
async fn backup_code_is_single_use() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("2fa-once");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Once", &email, password).await;
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
  let codes: Vec<String> = enable.json["backupCodes"]
    .as_array()
    .expect("backup codes")
    .iter()
    .map(|v| v.as_str().expect("code").to_string())
    .collect();
  assert_eq!(codes.len(), 10);

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

  let _ = json_post(
    &router,
    "/api/auth/sign-out",
    cookie.as_deref(),
    serde_json::json!({}),
  )
  .await;

  // First use consumes the code and signs in.
  let challenged = sign_in(&router, &email, password).await;
  assert_eq!(challenged.status, StatusCode::FORBIDDEN);
  let token = challenged.json["token"]
    .as_str()
    .expect("2fa token")
    .to_string();
  let used = json_post(
    &router,
    "/api/auth/two-factor/verify-backup-code",
    None,
    serde_json::json!({ "code": codes[0], "token": token }),
  )
  .await;
  assert_eq!(used.status, StatusCode::OK, "{:?}", used.json);

  // Same code with a fresh pending token must fail (single use).
  let challenged = sign_in(&router, &email, password).await;
  assert_eq!(challenged.status, StatusCode::FORBIDDEN);
  let token = challenged.json["token"]
    .as_str()
    .expect("2fa token")
    .to_string();
  let replay = json_post(
    &router,
    "/api/auth/two-factor/verify-backup-code",
    None,
    serde_json::json!({ "code": codes[0], "token": token }),
  )
  .await;
  assert_eq!(replay.status, StatusCode::UNAUTHORIZED, "{:?}", replay.json);

  // A different code still works with another fresh token.
  let challenged = sign_in(&router, &email, password).await;
  let token = challenged.json["token"]
    .as_str()
    .expect("2fa token")
    .to_string();
  let second = json_post(
    &router,
    "/api/auth/two-factor/verify-backup-code",
    None,
    serde_json::json!({ "code": codes[1], "token": token }),
  )
  .await;
  assert_eq!(second.status, StatusCode::OK, "{:?}", second.json);
}

/// Backup codes are Argon2id PHC only (newline-separated). Non-PHC rows
/// never verify.
#[tokio::test]
async fn backup_codes_are_phc_only() {
  use sqlx::Row;

  let (pool, router) = pool_and_router().await;
  let email = unique_email("2fa-phc-only");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "PhcOnly", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone();
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();

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

  // Stored column is newline-joined Argon2id PHC only.
  let row =
    sqlx::query("SELECT backup_codes FROM two_factor WHERE user_id = $1")
      .bind(user_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  let stored: String = row.get("backup_codes");
  assert!(!stored.contains(&backup), "plaintext must not be stored");
  let entries: Vec<&str> = stored
    .lines()
    .map(str::trim)
    .filter(|l| !l.is_empty())
    .collect();
  assert_eq!(entries.len(), 10);
  for entry in &entries {
    assert!(entry.starts_with("$argon2id$"), "PHC only, got: {entry}");
  }

  // A non-PHC row never verifies (fail hard, no SHA-256 fallback).
  sqlx::query("UPDATE two_factor SET backup_codes = $1 WHERE user_id = $2")
    .bind("00112233445566778899aabbccddeeff$fc21e2dc2665f7e70258365faaac64fd04b9d327395956095b4bfdc4982a492d")
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

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
  let rejected = json_post(
    &router,
    "/api/auth/two-factor/verify-backup-code",
    None,
    serde_json::json!({ "code": "1234-5678", "token": token }),
  )
  .await;
  assert_eq!(
    rejected.status,
    StatusCode::UNAUTHORIZED,
    "non-PHC backup code must fail hard: {:?}",
    rejected.json
  );
}

// --- P2-5: generate-backup-codes step-up (password or TOTP required) ---

/// Enable + verify 2FA for a fresh user; returns (session cookie, secret, user id).
async fn enable_verified_2fa(
  router: &topcoat::router::Router,
  prefix: &str,
  password: &str,
) -> (String, String, Uuid) {
  let email = unique_email(prefix);
  let signed_up = sign_up(router, "Step Up", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone().expect("cookie");
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();
  let enable = json_post(
    router,
    "/api/auth/two-factor/enable",
    Some(&cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);
  let secret = enable.json["secret"].as_str().expect("secret").to_string();
  let code = totp_code(&secret);
  let verified = json_post(
    router,
    "/api/auth/two-factor/verify-totp",
    Some(&cookie),
    serde_json::json!({ "code": code }),
  )
  .await;
  assert_eq!(verified.status, StatusCode::OK, "{:?}", verified.json);
  let cookie = verified.next_cookie.clone().unwrap_or(cookie);
  (cookie, secret, user_id)
}

#[tokio::test]
async fn generate_backup_codes_empty_body_is_401() {
  let (_pool, router) = pool_and_router().await;
  let password = "Harbor-Violet-Tiger-4821-Lantern!";
  let (cookie, _secret, _user_id) =
    enable_verified_2fa(&router, "2fa-gen-none", password).await;
  let res = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{:?}", res.json);
}

#[tokio::test]
async fn generate_backup_codes_wrong_password_is_401() {
  let (_pool, router) = pool_and_router().await;
  let password = "Harbor-Violet-Tiger-4821-Lantern!";
  let (cookie, _secret, _user_id) =
    enable_verified_2fa(&router, "2fa-gen-wrong", password).await;
  let res = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({ "password": "Wrong-Password-0000!" }),
  )
  .await;
  assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{:?}", res.json);
}

#[tokio::test]
async fn generate_backup_codes_correct_password_rotates_phc() {
  use sqlx::Row;

  let (pool, router) = pool_and_router().await;
  let password = "Harbor-Violet-Tiger-4821-Lantern!";
  let email_prefix = "2fa-gen-ok";
  let email = unique_email(email_prefix);
  let signed_up = sign_up(&router, "Step Up", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone().expect("cookie");
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();
  let enable = json_post(
    &router,
    "/api/auth/two-factor/enable",
    Some(&cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);
  let secret = enable.json["secret"].as_str().expect("secret").to_string();
  let old_code = enable.json["backupCodes"][0]
    .as_str()
    .expect("backup code")
    .to_string();
  let verified = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    Some(&cookie),
    serde_json::json!({ "code": totp_code(&secret) }),
  )
  .await;
  assert_eq!(verified.status, StatusCode::OK);
  let cookie = verified.next_cookie.clone().unwrap_or(cookie);

  let res = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({ "password": password }),
  )
  .await;
  assert_eq!(res.status, StatusCode::OK, "{:?}", res.json);
  let fresh: Vec<String> = res.json["backupCodes"]
    .as_array()
    .expect("backup codes")
    .iter()
    .map(|v| v.as_str().expect("code").to_string())
    .collect();
  assert_eq!(fresh.len(), 10);
  assert!(!fresh.contains(&old_code));

  // P2-2 Argon2 path: stored column is 10 PHC lines, no plaintext leak.
  let row =
    sqlx::query("SELECT backup_codes FROM two_factor WHERE user_id = $1")
      .bind(user_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  let stored: String = row.get("backup_codes");
  let entries: Vec<&str> = stored
    .lines()
    .map(str::trim)
    .filter(|l| !l.is_empty())
    .collect();
  assert_eq!(entries.len(), 10);
  for entry in &entries {
    assert!(entry.starts_with("$argon2id$"), "regen must write PHC");
  }
  for code in &fresh {
    assert!(!stored.contains(code), "plaintext must not be stored");
  }
}

#[tokio::test]
async fn generate_backup_codes_totp_only_for_passwordless_user() {
  let (pool, router) = pool_and_router().await;
  let password = "Harbor-Violet-Tiger-4821-Lantern!";
  let (cookie, secret, user_id) =
    enable_verified_2fa(&router, "2fa-gen-oauth", password).await;

  // Simulate an OAuth/passwordless user: drop the credential row so no
  // password can verify; the TOTP alternative must still allow rotation.
  sqlx::query(
    "DELETE FROM accounts WHERE user_id = $1 AND provider_id = 'credential'",
  )
  .bind(user_id)
  .execute(&pool)
  .await
  .expect("drop credential");

  // Password path now fails closed.
  let pw = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({ "password": password }),
  )
  .await;
  assert_eq!(pw.status, StatusCode::UNAUTHORIZED, "{:?}", pw.json);

  // TOTP-only (no password key at all) rotates.
  let res = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({ "code": totp_code_next_step(&secret) }),
  )
  .await;
  assert_eq!(res.status, StatusCode::OK, "{:?}", res.json);
  assert_eq!(res.json["backupCodes"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn generate_backup_codes_totp_replay_is_401() {
  let (_pool, router) = pool_and_router().await;
  let password = "Harbor-Violet-Tiger-4821-Lantern!";
  let (cookie, secret, _user_id) =
    enable_verified_2fa(&router, "2fa-gen-replay", password).await;

  let code = totp_code_next_step(&secret);
  let first = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({ "code": code }),
  )
  .await;
  assert_eq!(first.status, StatusCode::OK, "{:?}", first.json);

  let replay = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({ "code": code }),
  )
  .await;
  assert_eq!(replay.status, StatusCode::UNAUTHORIZED, "{:?}", replay.json);
}

#[tokio::test]
async fn generate_backup_codes_unverified_is_error() {
  let (_pool, router) = pool_and_router().await;
  let password = "Harbor-Violet-Tiger-4821-Lantern!";
  let email = unique_email("2fa-gen-unverified");
  let signed_up = sign_up(&router, "Unverified", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone().expect("cookie");
  let enable = json_post(
    &router,
    "/api/auth/two-factor/enable",
    Some(&cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);

  // 2FA enrolled but not verified: rotation must fail even with the
  // correct password (verified=true gate before rotation).
  let res = json_post(
    &router,
    "/api/auth/two-factor/generate-backup-codes",
    Some(&cookie),
    serde_json::json!({ "password": password }),
  )
  .await;
  assert_ne!(res.status, StatusCode::OK, "{:?}", res.json);
}
