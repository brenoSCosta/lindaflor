use http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use test_support::{
  json_get, json_post, pool_and_router, sign_in, sign_up, unique_email,
};
use topcoat::router::{Body, Router};
use uuid::Uuid;

async fn form_post(
  router: &Router,
  path: &str,
  cookie: Option<&str>,
  body: &str,
) -> (StatusCode, HeaderMap, String) {
  let mut builder = Request::builder().method(Method::POST).uri(path);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }
  let req = builder
    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
    .body(Body::from(body.to_owned()))
    .expect("build request");
  let response = router.handle(req).await;
  let status = response.status();
  let headers = response.headers().clone();
  let bytes = response
    .into_body()
    .collect()
    .await
    .expect("collect body")
    .to_bytes();
  (
    status,
    headers,
    String::from_utf8_lossy(&bytes).into_owned(),
  )
}

async fn get_page(
  router: &Router,
  path: &str,
  cookie: Option<&str>,
) -> (StatusCode, String) {
  let mut builder = Request::builder().method(Method::GET).uri(path);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }
  let req = builder.body(Body::empty()).expect("build request");
  let response = router.handle(req).await;
  let status = response.status();
  let bytes = response
    .into_body()
    .collect()
    .await
    .expect("collect body")
    .to_bytes();
  (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn list_revoke_session_and_revoke_others() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("sessions");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let first = sign_up(&router, "Sessions User", &email, password).await;
  assert_eq!(first.status, StatusCode::OK);
  let cookie_a = first.next_cookie.clone().expect("cookie a");

  // Second sign-in creates another DB session and a new cookie.
  let second = sign_in(&router, &email, password).await;
  assert_eq!(second.status, StatusCode::OK);
  let cookie_b = second.next_cookie.clone().expect("cookie b");

  let listed =
    json_get(&router, "/api/auth/list-sessions", Some(&cookie_b)).await;
  assert_eq!(listed.status, StatusCode::OK, "{:?}", listed.json);
  let sessions = listed.json.as_array().expect("sessions array");
  assert!(
    sessions.len() >= 2,
    "expected at least 2 sessions, got {:?}",
    sessions
  );

  let other_id = sessions
    .iter()
    .find(|s| s["current"] == false)
    .and_then(|s| s["id"].as_str())
    .expect("non-current session")
    .to_string();

  let revoked = json_post(
    &router,
    "/api/auth/revoke-session",
    Some(&cookie_b),
    serde_json::json!({ "sessionId": other_id }),
  )
  .await;
  assert_eq!(revoked.status, StatusCode::OK, "{:?}", revoked.json);

  // Old cookie should no longer resolve a session.
  let stale = json_get(&router, "/api/auth/get-session", Some(&cookie_a)).await;
  assert!(
    stale.json.is_null(),
    "revoked session cookie should be null, got {:?}",
    stale.json
  );

  // Create a third session, then revoke others from cookie_b.
  let third = sign_in(&router, &email, password).await;
  assert_eq!(third.status, StatusCode::OK);
  let cookie_c = third.next_cookie.clone().expect("cookie c");

  let revoke_others = json_post(
    &router,
    "/api/auth/revoke-other-sessions",
    Some(&cookie_c),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(
    revoke_others.status,
    StatusCode::OK,
    "{:?}",
    revoke_others.json
  );

  let only =
    json_get(&router, "/api/auth/list-sessions", Some(&cookie_c)).await;
  let remaining = only.json.as_array().unwrap();
  assert_eq!(remaining.len(), 1, "{:?}", remaining);
  assert_eq!(remaining[0]["current"], true);

  let old_b = json_get(&router, "/api/auth/get-session", Some(&cookie_b)).await;
  assert!(old_b.json.is_null(), "{:?}", old_b.json);
}

#[tokio::test]
async fn expired_temp_ban_lifts_for_active_session() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("ban-expiry");
  let password = "Harbor-Violet-Tiger-4821-Lantern!";

  let signed_up = sign_up(&router, "Ban Expiry", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone().expect("cookie");
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();

  // Active session resolves while no ban is set.
  let active = json_get(&router, "/api/auth/get-session", Some(&cookie)).await;
  assert_eq!(active.json["user"]["email"], email);

  // Expired temporary ban lifts: the active session still resolves
  // (`current_user` must honor `ban_expires`).
  sqlx::query!(
    r#"UPDATE users
       SET banned = true, ban_reason = 'temp', ban_expires = now() - interval '1 hour'
       WHERE id = $1"#,
    user_id,
  )
  .execute(&pool)
  .await
  .unwrap();
  let lifted = json_get(&router, "/api/auth/get-session", Some(&cookie)).await;
  assert_eq!(
    lifted.json["user"]["email"], email,
    "expired temp ban must lift for active sessions, got {:?}",
    lifted.json
  );

  // Unexpired temporary ban blocks the active session.
  sqlx::query!(
    r#"UPDATE users SET ban_expires = now() + interval '1 hour' WHERE id = $1"#,
    user_id,
  )
  .execute(&pool)
  .await
  .unwrap();
  let blocked = json_get(&router, "/api/auth/get-session", Some(&cookie)).await;
  assert!(
    blocked.json.is_null(),
    "unexpired temp ban must block active session, got {:?}",
    blocked.json
  );

  // Permanent ban blocks too.
  sqlx::query!(
    r#"UPDATE users SET ban_expires = NULL WHERE id = $1"#,
    user_id,
  )
  .execute(&pool)
  .await
  .unwrap();
  let banned = json_get(&router, "/api/auth/get-session", Some(&cookie)).await;
  assert!(
    banned.json.is_null(),
    "permanent ban must block active session, got {:?}",
    banned.json
  );

  // Lifting the ban restores the session.
  sqlx::query!(
    r#"UPDATE users
       SET banned = false, ban_reason = NULL, ban_expires = NULL
       WHERE id = $1"#,
    user_id,
  )
  .execute(&pool)
  .await
  .unwrap();
  let restored =
    json_get(&router, "/api/auth/get-session", Some(&cookie)).await;
  assert_eq!(restored.json["user"]["email"], email);

  // Sign-in expiry boundary: expired temp ban allows sign-in, future temp
  // ban denies it.
  sqlx::query!(
    r#"UPDATE users
       SET banned = true, ban_expires = now() - interval '1 minute'
       WHERE id = $1"#,
    user_id,
  )
  .execute(&pool)
  .await
  .unwrap();
  assert_eq!(
    sign_in(&router, &email, password).await.status,
    StatusCode::OK
  );
  sqlx::query!(
    r#"UPDATE users SET ban_expires = now() + interval '1 minute' WHERE id = $1"#,
    user_id,
  )
  .execute(&pool)
  .await
  .unwrap();
  assert_eq!(
    sign_in(&router, &email, password).await.status,
    StatusCode::UNAUTHORIZED
  );
}

/// Logging out must expire the cart cookie so the header badge falls back
/// to the guest state instead of stalling on the previous owner's count.
#[tokio::test]
async fn logout_expires_cart_cookie() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("logout-cart");

  let signed_up = sign_up(
    &router,
    "Logout Cart",
    &email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let session_cookie = signed_up.next_cookie.expect("session cookie");

  // Simulate a browser holding the user cart cookie from before logout.
  let cookie_header =
    format!("{session_cookie}; lindaflor_cart={}", Uuid::now_v7());
  let req = Request::builder()
    .method(Method::POST)
    .uri("/logout")
    .header(header::COOKIE, &cookie_header)
    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
    .body(Body::from("redirect=%2F"))
    .expect("build request");
  let response = router.handle(req).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER, "logout redirects");

  let set_cookies: Vec<String> = response
    .headers()
    .get_all(header::SET_COOKIE)
    .iter()
    .filter_map(|v| v.to_str().ok().map(str::to_owned))
    .collect();
  let cleared = test_support::find_set_cookie(&set_cookies, "lindaflor_cart")
    .expect("logout must expire the cart cookie");
  // Expiry + attribute parity with the write path (dev: no Secure; the
  // value is a secret, so failure messages stay redacted).
  test_support::assert_expired_hardened_cookie_dev(cleared, "lindaflor_cart");
}

const CHANGE_PW: &str = "Harbor-Violet-Tiger-4821-Lantern!";
const CHANGE_PW_NEW: &str = "Quilt-Mango-Brave-Pixel-7395?";

async fn two_cookies(
  router: &Router,
  prefix: &str,
) -> (String, String, String) {
  let email = unique_email(prefix);
  let first = sign_up(router, "Pw Sessions", &email, CHANGE_PW).await;
  assert_eq!(first.status, StatusCode::OK);
  let cookie_a = first.next_cookie.clone().expect("cookie a");
  let second = sign_in(router, &email, CHANGE_PW).await;
  assert_eq!(second.status, StatusCode::OK);
  let cookie_b = second.next_cookie.clone().expect("cookie b");
  (email, cookie_a, cookie_b)
}

/// Omitting `revokeOtherSessions` must revoke every other session (P2-4:
/// secure default). The current session keeps working and still reports
/// `current: true` in the session list.
#[tokio::test]
async fn change_password_defaults_to_revoking_other_sessions() {
  let (_pool, router) = pool_and_router().await;
  let (email, cookie_a, cookie_b) =
    two_cookies(&router, "pw-default-revoke").await;

  let changed = json_post(
    &router,
    "/api/auth/change-password",
    Some(&cookie_b),
    serde_json::json!({
        "currentPassword": CHANGE_PW,
        "newPassword": CHANGE_PW_NEW,
    }),
  )
  .await;
  assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.json);

  let stale = json_get(&router, "/api/auth/get-session", Some(&cookie_a)).await;
  assert!(
    stale.json.is_null(),
    "other session must be revoked by default, got {:?}",
    stale.json
  );

  let current =
    json_get(&router, "/api/auth/get-session", Some(&cookie_b)).await;
  assert_eq!(current.json["user"]["email"], email);

  let only =
    json_get(&router, "/api/auth/list-sessions", Some(&cookie_b)).await;
  let remaining = only.json.as_array().expect("sessions array");
  assert_eq!(remaining.len(), 1, "{:?}", remaining);
  assert_eq!(remaining[0]["current"], true);
}

/// An explicit `revokeOtherSessions: false` opts out and keeps the other
/// session alive.
#[tokio::test]
async fn change_password_explicit_false_keeps_other_sessions() {
  let (_pool, router) = pool_and_router().await;
  let (email, cookie_a, cookie_b) =
    two_cookies(&router, "pw-keep-others").await;

  let changed = json_post(
    &router,
    "/api/auth/change-password",
    Some(&cookie_b),
    serde_json::json!({
        "currentPassword": CHANGE_PW,
        "newPassword": CHANGE_PW_NEW,
        "revokeOtherSessions": false,
    }),
  )
  .await;
  assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.json);

  let kept_a =
    json_get(&router, "/api/auth/get-session", Some(&cookie_a)).await;
  assert_eq!(kept_a.json["user"]["email"], email);
  let kept_b =
    json_get(&router, "/api/auth/get-session", Some(&cookie_b)).await;
  assert_eq!(kept_b.json["user"]["email"], email);
}

/// HTML settings parity: the change-password form defaults to revoking
/// (field absent) and honors the `revoke_other_sessions=false` opt-out
/// checkbox value.
#[tokio::test]
async fn settings_change_password_parity_with_api_default() {
  let _ = lindaflor::app::settings::page;
  let _ = lindaflor::app::settings::settings_post;

  // Default (no flag field): other session revoked.
  let (_pool, router) = pool_and_router().await;
  let (email, cookie_a, cookie_b) =
    two_cookies(&router, "pw-html-default").await;
  let (status, _, _) = form_post(
    &router,
    "/settings?tab=account",
    Some(&cookie_b),
    &format!(
      "action=change_password&current_password={CHANGE_PW}\
       &new_password={CHANGE_PW_NEW}&confirm_password={CHANGE_PW_NEW}"
    ),
  )
  .await;
  assert!(
    status.is_redirection(),
    "settings POST must redirect, got {status}"
  );
  let stale = json_get(&router, "/api/auth/get-session", Some(&cookie_a)).await;
  assert!(
    stale.json.is_null(),
    "HTML default must revoke other sessions, got {:?}",
    stale.json
  );
  let current =
    json_get(&router, "/api/auth/get-session", Some(&cookie_b)).await;
  assert_eq!(current.json["user"]["email"], email);

  // Explicit opt-out: both sessions survive.
  let (_pool, router) = pool_and_router().await;
  let (email, cookie_a, cookie_b) = two_cookies(&router, "pw-html-keep").await;
  let (status, _, _) = form_post(
    &router,
    "/settings?tab=account",
    Some(&cookie_b),
    &format!(
      "action=change_password&current_password={CHANGE_PW}\
       &new_password={CHANGE_PW_NEW}&confirm_password={CHANGE_PW_NEW}\
       &revoke_other_sessions=false"
    ),
  )
  .await;
  assert!(
    status.is_redirection(),
    "settings POST must redirect, got {status}"
  );
  let kept_a =
    json_get(&router, "/api/auth/get-session", Some(&cookie_a)).await;
  assert_eq!(kept_a.json["user"]["email"], email);
  let kept_b =
    json_get(&router, "/api/auth/get-session", Some(&cookie_b)).await;
  assert_eq!(kept_b.json["user"]["email"], email);
}

/// The sessions tab offers a global "sign out everywhere" action that
/// revokes every session except the current one (which keeps no button).
#[tokio::test]
async fn settings_revoke_other_sessions_action() {
  let _ = lindaflor::app::settings::page;
  let _ = lindaflor::app::settings::settings_post;

  let (_pool, router) = pool_and_router().await;
  let (email, cookie_a, cookie_b) =
    two_cookies(&router, "signout-everywhere").await;

  let (status, body) =
    get_page(&router, "/settings?tab=sessions", Some(&cookie_b)).await;
  assert_eq!(status, StatusCode::OK, "{body}");
  assert!(
    body.contains("revoke_other_sessions"),
    "sessions tab must render the sign-out-everywhere action, got: {body}"
  );
  assert!(
    body.contains("Sair de todos"),
    "sessions tab must label the global sign-out button, got: {body}"
  );

  let (status, _, _) = form_post(
    &router,
    "/settings?tab=sessions",
    Some(&cookie_b),
    "action=revoke_other_sessions",
  )
  .await;
  assert!(
    status.is_redirection(),
    "settings POST must redirect, got {status}"
  );

  let stale = json_get(&router, "/api/auth/get-session", Some(&cookie_a)).await;
  assert!(
    stale.json.is_null(),
    "other session must be revoked, got {:?}",
    stale.json
  );
  let current =
    json_get(&router, "/api/auth/get-session", Some(&cookie_b)).await;
  assert_eq!(current.json["user"]["email"], email);

  let only =
    json_get(&router, "/api/auth/list-sessions", Some(&cookie_b)).await;
  let remaining = only.json.as_array().expect("sessions array");
  assert_eq!(remaining.len(), 1, "{:?}", remaining);
  assert_eq!(remaining[0]["current"], true);
}
