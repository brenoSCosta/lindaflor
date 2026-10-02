mod common;

use common::{
  json_get, json_post, pool_and_router, sign_in, sign_up, unique_email,
};
use http::StatusCode;

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
