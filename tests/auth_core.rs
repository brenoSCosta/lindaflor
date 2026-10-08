use http::StatusCode;
use test_support::{
  json_get, json_post, pool_and_router, sign_in, sign_up, unique_email,
};
use uuid::Uuid;

#[tokio::test]
async fn sign_up_sign_in_get_session_sign_out() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("core");
  let password = "password123";

  let signed_up = sign_up(&router, "Core User", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK, "{:?}", signed_up.json);
  assert!(
    signed_up.cookie.is_some(),
    "sign-up should set session cookie"
  );
  assert_eq!(signed_up.json["user"]["email"], email);
  let cookie = signed_up.next_cookie.as_deref();

  let session = json_get(&router, "/api/auth/get-session", cookie).await;
  assert_eq!(session.status, StatusCode::OK);
  assert_eq!(session.json["user"]["email"], email);
  assert!(session.json["session"]["id"].is_string());

  let out =
    json_post(&router, "/api/auth/sign-out", cookie, serde_json::json!({}))
      .await;
  assert_eq!(out.status, StatusCode::OK);
  assert_eq!(out.json["success"], true);

  let after =
    json_get(&router, "/api/auth/get-session", out.next_cookie.as_deref())
      .await;
  assert_eq!(after.status, StatusCode::OK);
  assert!(
    after.json.is_null(),
    "expected null session, got {:?}",
    after.json
  );

  let again = sign_in(&router, &email, password).await;
  assert_eq!(again.status, StatusCode::OK, "{:?}", again.json);
  assert!(again.cookie.is_some());
  assert_eq!(again.json["user"]["email"], email);
}

#[tokio::test]
async fn sign_in_wrong_password_is_unauthorized() {
  let (_pool, router) = pool_and_router().await;
  let email = unique_email("wrong-pw");
  let signed_up = sign_up(&router, "Wrong Pw", &email, "password123").await;
  assert_eq!(signed_up.status, StatusCode::OK);

  let bad = sign_in(&router, &email, "not-the-password").await;
  assert_eq!(bad.status, StatusCode::UNAUTHORIZED, "{:?}", bad.json);
}

#[tokio::test]
async fn expired_temp_ban_does_not_block_sign_in_or_session() {
  let (pool, router) = pool_and_router().await;
  let email = unique_email("core-ban-expiry");
  let password = "password123";

  let signed_up = sign_up(&router, "Core Ban Expiry", &email, password).await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let cookie = signed_up.next_cookie.clone().expect("cookie");
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();

  sqlx::query(
    "UPDATE users
     SET banned = true, ban_reason = 'temp',
         ban_expires = now() - interval '1 hour'
     WHERE id = $1",
  )
  .bind(user_id)
  .execute(&pool)
  .await
  .unwrap();

  let session = json_get(&router, "/api/auth/get-session", Some(&cookie)).await;
  assert_eq!(
    session.json["user"]["email"], email,
    "expired temp ban must lift for current_user, got {:?}",
    session.json
  );

  let again = sign_in(&router, &email, password).await;
  assert_eq!(
    again.status,
    StatusCode::OK,
    "expired temp ban must not block sign-in, got {:?}",
    again.json
  );
}
