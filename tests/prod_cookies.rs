//! Production cookie hardening (P2-7).
//!
//! Prod-only binary: every test here builds its router with
//! `pool_and_prod_router` (`APP_ENV=production`). Never add dev-router tests
//! to this file — cookie attributes are read from process-global env per
//! request, so mixing dev and prod routers in one binary races.
//!
//! Cookie values are secrets: failure messages use redacted lines only (see
//! `test_support::redact_set_cookie`).

use http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use test_support::{
  PROD_CART_COOKIE, PROD_PENDING_2FA_COOKIE, PROD_SESSION_COOKIE,
  PROD_TOAST_COOKIE, assert_expired_hardened_cookie, assert_hardened_cookie,
  find_set_cookie, json_post, pool_and_prod_router, set_cookie_headers,
  sign_up, totp_code, unique_email,
};
use topcoat::router::{Body, Router};
use uuid::Uuid;

async fn raw(
  router: &Router,
  method: Method,
  path: &str,
  cookie: Option<&str>,
  content_type: Option<&str>,
  body: Vec<u8>,
) -> (StatusCode, Vec<String>, Vec<u8>) {
  let mut builder = Request::builder().method(method).uri(path);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }
  if let Some(content_type) = content_type {
    builder = builder.header(header::CONTENT_TYPE, content_type);
  }
  let req = builder.body(Body::from(body)).expect("build request");
  let response = router.handle(req).await;
  let status = response.status();
  let cookies = set_cookie_headers(&response);
  let bytes = response
    .into_body()
    .collect()
    .await
    .expect("collect body")
    .to_bytes()
    .to_vec();
  (status, cookies, bytes)
}

fn require_cookie<'a>(
  cookies: &'a [String],
  name: &str,
  context: &str,
) -> &'a str {
  find_set_cookie(cookies, name).unwrap_or_else(|| {
    let names: Vec<&str> = cookies
      .iter()
      .map(|c| c.split_once(';').map(|(first, _)| first).unwrap_or(c))
      .collect();
    panic!("{context}: expected Set-Cookie {name}=, got names {names:?}")
  })
}

fn assert_no_bare_cookie(cookies: &[String], bare: &str, context: &str) {
  assert!(
    find_set_cookie(cookies, bare).is_none(),
    "{context}: production must not set the bare {bare} cookie"
  );
}

fn form_value(raw: &str) -> String {
  // Test emails/passwords are URL-safe except `@`.
  raw.replace('@', "%40")
}

/// Pending-2FA, session, cart and toast cookies must all be `__Host-`
/// prefixed with `Secure` (+ `HttpOnly`, `SameSite=Lax`, `Path=/`) in
/// production, and the bare dev names must be absent.
#[tokio::test]
async fn prod_cookies_secure() {
  let (_pool, router) = pool_and_prod_router().await;
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  // --- Session + cart + toast on HTML sign-up ---
  // `signup_post` → `service::sign_up_email` → `start_session_for_user`
  // writes the session and cart cookies; the success toast is queued too.
  // `next=/login` keeps the redirect off unmounted module pages (the test
  // router only mounts `#[route]`s plus the pages in `build_prod_router`).
  let email = unique_email("prod-cookies");
  let form = form_value(&format!(
    "name=Prod%20Cookies&email={email}&password={password}&next=/login"
  ));
  let (status, cookies, _) = raw(
    &router,
    Method::POST,
    "/signup",
    None,
    Some("application/x-www-form-urlencoded"),
    form.into_bytes(),
  )
  .await;
  assert!(
    status.is_redirection(),
    "sign-up must redirect, got {status}"
  );
  let session = require_cookie(&cookies, PROD_SESSION_COOKIE, "sign-up");
  assert_hardened_cookie(session, PROD_SESSION_COOKIE);
  assert_no_bare_cookie(&cookies, "session", "sign-up");
  let cart = require_cookie(&cookies, PROD_CART_COOKIE, "sign-up");
  assert_hardened_cookie(cart, PROD_CART_COOKIE);
  assert_no_bare_cookie(&cookies, "lindaflor_cart", "sign-up");
  let toast = require_cookie(&cookies, PROD_TOAST_COOKIE, "sign-up");
  assert_hardened_cookie(toast, PROD_TOAST_COOKIE);
  assert_no_bare_cookie(&cookies, "toast", "sign-up");

  // --- Pending-2FA via the HTML login flow ---
  // Enable + verify 2FA on a dedicated user, then hit the HTML login form
  // (its challenge path is the only writer of the pending cookie).
  let email_2fa = unique_email("prod-pending");
  let created = sign_up(&router, "Prod Pending", &email_2fa, password).await;
  assert_eq!(created.status, StatusCode::OK);
  let session_cookie = created.next_cookie.clone().expect("session cookie");
  let enable = json_post(
    &router,
    "/api/auth/two-factor/enable",
    Some(&session_cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(enable.status, StatusCode::OK, "{:?}", enable.json);
  let secret = enable.json["secret"].as_str().expect("secret").to_owned();
  let verified = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    Some(&session_cookie),
    serde_json::json!({ "code": totp_code(&secret) }),
  )
  .await;
  assert_eq!(verified.status, StatusCode::OK, "{:?}", verified.json);
  let _ = json_post(
    &router,
    "/api/auth/sign-out",
    Some(&session_cookie),
    serde_json::json!({}),
  )
  .await;

  // `next=/login` keeps `safe_next` off the unmounted dashboard page; the
  // 2FA branch ignores it and redirects to the two-factor page instead.
  let form = form_value(&format!(
    "email={email_2fa}&password={password}&next=/login"
  ));
  let (status, cookies, _) = raw(
    &router,
    Method::POST,
    "/login",
    None,
    Some("application/x-www-form-urlencoded"),
    form.into_bytes(),
  )
  .await;
  assert!(
    status.is_redirection(),
    "2fa login must redirect, got {status}"
  );
  let pending =
    require_cookie(&cookies, PROD_PENDING_2FA_COOKIE, "2fa challenge");
  assert_hardened_cookie(pending, PROD_PENDING_2FA_COOKIE);
  assert_no_bare_cookie(&cookies, "lindaflor_pending_2fa", "2fa challenge");

  // --- Toast on a failed HTML login ---
  // The error redirect targets the (mounted) login page itself; `next`
  // only feeds `safe_next`, which must avoid the unmounted dashboard page.
  let bad_form = form_value(
    "email=prod-toast@example.com&password=wrong-password-1!&next=/login",
  );
  let (status, cookies, _) = raw(
    &router,
    Method::POST,
    "/login",
    None,
    Some("application/x-www-form-urlencoded"),
    bad_form.into_bytes(),
  )
  .await;
  assert!(
    status.is_redirection(),
    "failed login must redirect, got {status}"
  );
  let toast = require_cookie(&cookies, PROD_TOAST_COOKIE, "failed login");
  assert_hardened_cookie(toast, PROD_TOAST_COOKIE);
  assert_no_bare_cookie(&cookies, "toast", "failed login");
}

/// Signing out via the API must expire the `__Host-` session cookie with
/// matching attributes (not just `Max-Age=0` on a bare name).
#[tokio::test]
async fn prod_sign_out_expires_hardened_session_cookie() {
  let (_pool, router) = pool_and_prod_router().await;
  let created = sign_up(
    &router,
    "Prod Signout",
    &unique_email("prod-signout"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(created.status, StatusCode::OK);
  let session_cookie = created.next_cookie.clone().expect("session cookie");

  let (status, cookies, _) = raw(
    &router,
    Method::POST,
    "/api/auth/sign-out",
    Some(&session_cookie),
    Some("application/json"),
    serde_json::to_vec(&serde_json::json!({})).expect("serialize body"),
  )
  .await;
  assert_eq!(status, StatusCode::OK, "sign-out must succeed");
  let cleared = require_cookie(&cookies, PROD_SESSION_COOKIE, "sign-out");
  assert_expired_hardened_cookie(cleared, PROD_SESSION_COOKIE);
}

/// HTML logout must expire both the session and the cart `__Host-` cookies
/// with matching attributes.
#[tokio::test]
async fn prod_logout_expires_hardened_cookies() {
  let (_pool, router) = pool_and_prod_router().await;
  let created = sign_up(
    &router,
    "Prod Logout",
    &unique_email("prod-logout"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(created.status, StatusCode::OK);
  let session_cookie = created.next_cookie.expect("session cookie");

  // Simulate a browser holding the prod cart cookie from before logout.
  let cookie_header =
    format!("{session_cookie}; {PROD_CART_COOKIE}={}", Uuid::now_v7());
  let (status, cookies, _) = raw(
    &router,
    Method::POST,
    "/logout",
    Some(&cookie_header),
    Some("application/x-www-form-urlencoded"),
    "redirect=%2F".as_bytes().to_vec(),
  )
  .await;
  assert_eq!(status, StatusCode::SEE_OTHER, "logout must redirect");
  let cleared_cart = require_cookie(&cookies, PROD_CART_COOKIE, "logout");
  assert_expired_hardened_cookie(cleared_cart, PROD_CART_COOKIE);
  let cleared_session = require_cookie(&cookies, PROD_SESSION_COOKIE, "logout");
  assert_expired_hardened_cookie(cleared_session, PROD_SESSION_COOKIE);
}
