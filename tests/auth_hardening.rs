//! Security-review fixes A3 (step-up + impersonation), A5 (TOTP cap) and
//! A6 (enumeration).

use http::{Method, Request, StatusCode, header};
use sqlx::PgPool;
use test_support::{
  json_post, pool_and_router, sign_in, sign_up, totp_code, unique_email,
};
use topcoat::router::{Body, Router};
use uuid::Uuid;

const PASSWORD: &str = "Harbor-Violet-Tiger-4821-Lantern!";
const PASSWORD_FORM: &str = "Harbor-Violet-Tiger-4821-Lantern%21";

struct Account {
  cookie: String,
  id: Uuid,
  email: String,
}

async fn account(router: &Router, prefix: &str) -> Account {
  let email = unique_email(prefix);
  let res = sign_up(router, "Hardening User", &email, PASSWORD).await;
  assert_eq!(res.status, StatusCode::OK, "{:?}", res.json);
  Account {
    cookie: res.next_cookie.clone().expect("cookie"),
    id: Uuid::parse_str(res.json["user"]["id"].as_str().unwrap()).unwrap(),
    email,
  }
}

async fn make_admin(pool: &PgPool, id: Uuid) {
  sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

async fn settings_post(
  router: &Router,
  cookie: &str,
  tab: &str,
  body: &str,
) -> StatusCode {
  let req = Request::builder()
    .method(Method::POST)
    .uri(format!("/settings?tab={tab}"))
    .header(header::COOKIE, cookie)
    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
    .body(Body::from(body.to_owned()))
    .expect("build request");
  router.handle(req).await.status()
}

async fn change_email_tokens(pool: &PgPool, user_id: Uuid) -> i64 {
  sqlx::query_scalar(
    "SELECT COUNT(*) FROM verifications WHERE identifier LIKE $1",
  )
  .bind(format!("change-email:{user_id}:%"))
  .fetch_one(pool)
  .await
  .unwrap()
}

async fn audit_count(pool: &PgPool, aggregate_id: Uuid, action: &str) -> i64 {
  sqlx::query_scalar(
    "SELECT COUNT(*) FROM audit_events
     WHERE aggregate_id = $1 AND metadata->>'action' = $2",
  )
  .bind(aggregate_id.to_string())
  .bind(action)
  .fetch_one(pool)
  .await
  .unwrap()
}

/// Enroll + verify TOTP for `acct`; returns the base32 secret.
async fn enable_2fa(router: &Router, acct: &Account) -> String {
  let enable = json_post(
    router,
    "/api/auth/two-factor/enable",
    Some(&acct.cookie),
    serde_json::json!({ "password": PASSWORD }),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);
  let secret = enable.json["secret"].as_str().unwrap().to_owned();
  let verified = json_post(
    router,
    "/api/auth/two-factor/verify-totp",
    Some(&acct.cookie),
    serde_json::json!({ "code": totp_code(&secret) }),
  )
  .await;
  assert_eq!(verified.status, StatusCode::OK, "{:?}", verified.json);
  secret
}

/// A six-digit code that is not `valid`.
fn wrong_code(valid: &str) -> String {
  let n: u32 = valid.parse().unwrap();
  format!("{:06}", (n + 500_000) % 1_000_000)
}

// --- A3: impersonation ---

#[tokio::test]
async fn impersonation_is_audited_capped_and_cannot_change_credentials() {
  let (pool, router) = pool_and_router().await;
  let admin = account(&router, "imp-admin").await;
  make_admin(&pool, admin.id).await;
  let target = account(&router, "imp-target").await;

  let imp = json_post(
    &router,
    "/api/auth/admin/impersonate-user",
    Some(&admin.cookie),
    serde_json::json!({ "userId": target.id.to_string() }),
  )
  .await;
  assert_eq!(imp.status, StatusCode::OK, "{:?}", imp.json);
  let imp_cookie = imp.next_cookie.clone().expect("impersonation cookie");
  assert_eq!(audit_count(&pool, target.id, "impersonate_start").await, 1);

  // The impersonation row expires within an hour, not the normal lifetime.
  let capped: bool = sqlx::query_scalar(
    "SELECT bool_and(expires_at <= (now() AT TIME ZONE 'UTC') + interval '61 minutes')
     FROM sessions WHERE user_id = $1 AND impersonated_by = $2",
  )
  .bind(target.id)
  .bind(admin.id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert!(capped, "impersonation session must be capped at 1h");

  let forbidden_calls = [
    (
      "/api/auth/change-email",
      serde_json::json!({ "newEmail": unique_email("imp-new"), "password": PASSWORD }),
    ),
    (
      "/api/auth/link-social",
      serde_json::json!({ "provider": "google", "password": PASSWORD }),
    ),
    (
      "/api/auth/two-factor/enable",
      serde_json::json!({ "password": PASSWORD }),
    ),
    (
      "/api/auth/change-password",
      serde_json::json!({
          "currentPassword": PASSWORD,
          "newPassword": "Quilt-Mango-Brave-Pixel-7395?",
      }),
    ),
    (
      "/api/auth/unlink-account",
      serde_json::json!({ "providerId": "google", "accountId": "x", "password": PASSWORD }),
    ),
    (
      "/api/auth/update-user",
      serde_json::json!({ "name": "Hijacked" }),
    ),
    ("/api/auth/revoke-other-sessions", serde_json::json!({})),
    (
      "/api/auth/delete-user",
      serde_json::json!({ "password": PASSWORD }),
    ),
  ];
  for (path, body) in forbidden_calls {
    let res = json_post(&router, path, Some(&imp_cookie), body).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}: {:?}", res.json);
  }

  // HTML settings refuse too, even with the right password.
  let status = settings_post(
    &router,
    &imp_cookie,
    "account",
    &format!(
      "action=change_email&new_email={}&password={PASSWORD_FORM}",
      unique_email("imp-html").replace('@', "%40")
    ),
  )
  .await;
  assert!(status.is_redirection(), "{status}");
  assert_eq!(change_email_tokens(&pool, target.id).await, 0);

  let stopped = json_post(
    &router,
    "/api/auth/admin/stop-impersonating",
    Some(&imp_cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(stopped.status, StatusCode::OK, "{:?}", stopped.json);
  assert_eq!(audit_count(&pool, target.id, "impersonate_stop").await, 1);
  let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .unwrap();
  assert_eq!(name, "Hardening User");
}

#[tokio::test]
async fn admin_rename_is_audited() {
  let (pool, router) = pool_and_router().await;
  let admin = account(&router, "rename-admin").await;
  make_admin(&pool, admin.id).await;
  let target = account(&router, "rename-target").await;

  let renamed = json_post(
    &router,
    "/api/auth/admin/update-user",
    Some(&admin.cookie),
    serde_json::json!({ "userId": target.id.to_string(), "name": "Novo Nome" }),
  )
  .await;
  assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.json);
  assert_eq!(audit_count(&pool, target.id, "rename_user").await, 1);
}

// --- A3: step-up ---

#[tokio::test]
async fn enabling_2fa_requires_password() {
  let (_pool, router) = pool_and_router().await;
  let acct = account(&router, "stepup-enable").await;

  for body in [
    serde_json::json!({}),
    serde_json::json!({ "password": "wrong-password" }),
  ] {
    let res = json_post(
      &router,
      "/api/auth/two-factor/enable",
      Some(&acct.cookie),
      body,
    )
    .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{:?}", res.json);
  }
  let ok = json_post(
    &router,
    "/api/auth/two-factor/enable",
    Some(&acct.cookie),
    serde_json::json!({ "password": PASSWORD }),
  )
  .await;
  assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.json);
}

#[tokio::test]
async fn unlink_requires_step_up() {
  let (pool, router) = pool_and_router().await;
  let acct = account(&router, "stepup-unlink").await;
  sqlx::query(
    "INSERT INTO accounts (id, account_id, provider_id, user_id, created_at, updated_at)
     VALUES ($1, $2, 'google', $3, now(), now())",
  )
  .bind(Uuid::now_v7())
  .bind(format!("google-{}", acct.id))
  .bind(acct.id)
  .execute(&pool)
  .await
  .unwrap();

  let body = |password: Option<&str>| {
    serde_json::json!({
        "providerId": "google",
        "accountId": format!("google-{}", acct.id),
        "password": password,
    })
  };
  let denied = json_post(
    &router,
    "/api/auth/unlink-account",
    Some(&acct.cookie),
    body(None),
  )
  .await;
  assert_eq!(denied.status, StatusCode::UNAUTHORIZED, "{:?}", denied.json);
  let ok = json_post(
    &router,
    "/api/auth/unlink-account",
    Some(&acct.cookie),
    body(Some(PASSWORD)),
  )
  .await;
  assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.json);
}

#[tokio::test]
async fn html_change_email_requires_step_up_before_revoking_sessions() {
  let (pool, router) = pool_and_router().await;
  let acct = account(&router, "stepup-html").await;
  let other = sign_in(&router, &acct.email, PASSWORD).await;
  let other_cookie = other.next_cookie.clone().expect("second session");
  let new_email = unique_email("stepup-html-new").replace('@', "%40");

  // Session cookie alone: refused, nothing queued, other session intact.
  let status = settings_post(
    &router,
    &acct.cookie,
    "account",
    &format!("action=change_email&new_email={new_email}"),
  )
  .await;
  assert!(status.is_redirection(), "{status}");
  assert_eq!(change_email_tokens(&pool, acct.id).await, 0);
  let still = test_support::json_get(
    &router,
    "/api/auth/get-session",
    Some(&other_cookie),
  )
  .await;
  assert_eq!(still.json["user"]["email"], acct.email);

  // With the password it goes through.
  let status = settings_post(
    &router,
    &acct.cookie,
    "account",
    &format!(
      "action=change_email&new_email={new_email}&password={PASSWORD_FORM}"
    ),
  )
  .await;
  assert!(status.is_redirection(), "{status}");
  assert_eq!(change_email_tokens(&pool, acct.id).await, 1);
}

// --- A5: per-user TOTP cap + replay ---

#[tokio::test]
async fn wrong_totp_codes_lock_the_user_across_paths() {
  let (_pool, router) = pool_and_router().await;
  let acct = account(&router, "totp-cap").await;
  let secret = enable_2fa(&router, &acct).await;

  // Step-up path (disable) burns the per-user budget…
  let bad = wrong_code(&totp_code(&secret));
  for _ in 0..lindaflor::valkey::TOTP_FAIL_MAX {
    let res = json_post(
      &router,
      "/api/auth/two-factor/disable",
      Some(&acct.cookie),
      serde_json::json!({ "code": bad }),
    )
    .await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{:?}", res.json);
  }

  // …so a fresh pending token with a known password cannot reset it.
  let challenged = sign_in(&router, &acct.email, PASSWORD).await;
  assert_eq!(challenged.json["code"], "TWO_FACTOR_REQUIRED");
  let token = challenged.json["token"].as_str().unwrap();
  let locked = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    None,
    serde_json::json!({ "code": totp_code(&secret), "token": token }),
  )
  .await;
  assert_eq!(
    locked.status,
    StatusCode::TOO_MANY_REQUESTS,
    "{:?}",
    locked.json
  );
  assert!(locked.cookie.is_none());
}

#[tokio::test]
async fn step_up_totp_cannot_be_replayed() {
  let (_pool, router) = pool_and_router().await;
  let acct = account(&router, "totp-replay").await;
  let secret = enable_2fa(&router, &acct).await;
  // Next-step code: accepted by skew=1 and never presented before.
  let code = totp_code_next_step(&secret);

  let first = json_post(
    &router,
    "/api/auth/change-email",
    Some(&acct.cookie),
    serde_json::json!({ "newEmail": unique_email("totp-replay-a"), "code": code }),
  )
  .await;
  assert_eq!(first.status, StatusCode::OK, "{:?}", first.json);

  let replay = json_post(
    &router,
    "/api/auth/change-email",
    Some(&acct.cookie),
    serde_json::json!({ "newEmail": unique_email("totp-replay-b"), "code": code }),
  )
  .await;
  assert_eq!(replay.status, StatusCode::UNAUTHORIZED, "{:?}", replay.json);
}

fn totp_code_next_step(secret_b32: &str) -> String {
  use std::time::{SystemTime, UNIX_EPOCH};
  use totp_rs::{Algorithm, Builder, Secret};

  let totp = Builder::new()
    .with_algorithm(Algorithm::SHA1)
    .with_digits(6)
    .with_skew(1)
    .with_step_duration(30)
    .with_secret(Secret::try_from_base32(secret_b32).expect("base32 secret"))
    .build()
    .expect("totp builder");
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .expect("clock")
    .as_secs();
  totp.generate(now + 30).to_string()
}

// --- A6: enumeration ---

#[tokio::test]
async fn unknown_and_wrong_password_sign_in_look_the_same() {
  let (_pool, router) = pool_and_router().await;
  let acct = account(&router, "enum").await;
  let wrong = sign_in(&router, &acct.email, "not-the-password").await;
  let unknown =
    sign_in(&router, &unique_email("enum-missing"), "not-the-password").await;
  assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
  assert_eq!(unknown.status, wrong.status);
  assert_eq!(unknown.json, wrong.json);
}
