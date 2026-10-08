use http::{Method, Request, StatusCode, header};
use test_support::{
  json_get, json_post, pool_and_router, sign_in, sign_up, unique_email,
};
use topcoat::router::Body;
use uuid::Uuid;

#[tokio::test]
async fn list_revoke_session_and_revoke_others() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("sessions");
  let password = "password123";

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
  let password = "password123";

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

  let signed_up = sign_up(&router, "Logout Cart", &email, "password123").await;
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
  assert!(
    set_cookies.iter().any(|v| {
      v.starts_with("lindaflor_cart=")
        && (v.contains("Max-Age=0") || v.contains("1970"))
    }),
    "logout must expire the cart cookie, got {set_cookies:?}"
  );
}
