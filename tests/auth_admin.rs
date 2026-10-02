mod common;

use common::{json_get, json_post, pool_and_router, sign_up, unique_email};
use http::StatusCode;
use uuid::Uuid;

#[tokio::test]
async fn admin_impersonate_and_stop() {
  let (pool, router) = pool_and_router().await;

  let admin_email = unique_email("admin");
  let target_email = unique_email("target");

  let admin = sign_up(&router, "Admin User", &admin_email, "password123").await;
  assert_eq!(admin.status, StatusCode::OK);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  let admin_id =
    Uuid::parse_str(admin.json["user"]["id"].as_str().unwrap()).unwrap();

  sqlx::query!(r#"UPDATE users SET role = 'admin' WHERE id = $1"#, admin_id)
    .execute(&pool)
    .await
    .unwrap();

  let target =
    sign_up(&router, "Target User", &target_email, "password123").await;
  assert_eq!(target.status, StatusCode::OK);
  let target_id = target.json["user"]["id"].as_str().unwrap().to_string();

  let impersonated = json_post(
    &router,
    "/api/auth/admin/impersonate-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id }),
  )
  .await;
  assert_eq!(
    impersonated.status,
    StatusCode::OK,
    "{:?}",
    impersonated.json
  );
  assert_eq!(impersonated.json["user"]["email"], target_email);
  assert_eq!(
    impersonated.json["session"]["impersonatedBy"],
    admin_id.to_string()
  );
  let imp_cookie = impersonated
    .next_cookie
    .clone()
    .expect("impersonation cookie");

  let session =
    json_get(&router, "/api/auth/get-session", Some(&imp_cookie)).await;
  assert_eq!(session.status, StatusCode::OK);
  assert_eq!(session.json["user"]["email"], target_email);
  assert_eq!(
    session.json["session"]["impersonatedBy"],
    admin_id.to_string()
  );

  let stopped = json_post(
    &router,
    "/api/auth/admin/stop-impersonating",
    Some(&imp_cookie),
    serde_json::json!({}),
  )
  .await;
  assert_eq!(stopped.status, StatusCode::OK, "{:?}", stopped.json);
  assert_eq!(stopped.json["user"]["email"], admin_email);
  assert!(stopped.json["session"]["impersonatedBy"].is_null());
}

#[tokio::test]
async fn non_admin_cannot_impersonate() {
  let (_pool, router) = pool_and_router().await;

  let user =
    sign_up(&router, "Regular", &unique_email("nonadmin"), "password123").await;
  assert_eq!(user.status, StatusCode::OK);
  let cookie = user.next_cookie.clone();

  let other =
    sign_up(&router, "Other", &unique_email("other"), "password123").await;
  let other_id = other.json["user"]["id"].as_str().unwrap();

  let denied = json_post(
    &router,
    "/api/auth/admin/impersonate-user",
    cookie.as_deref(),
    serde_json::json!({ "userId": other_id }),
  )
  .await;
  assert_eq!(denied.status, StatusCode::FORBIDDEN, "{:?}", denied.json);
}

fn user_id(json: &serde_json::Value) -> Uuid {
  Uuid::parse_str(json["user"]["id"].as_str().unwrap()).unwrap()
}

async fn promote(pool: &sqlx::PgPool, id: Uuid, role: &str) {
  sqlx::query!(r#"UPDATE users SET role = $2 WHERE id = $1"#, id, role)
    .execute(pool)
    .await
    .unwrap();
}

async fn session_count(pool: &sqlx::PgPool, user_id: Uuid) -> i64 {
  sqlx::query_scalar!(
    r#"SELECT COUNT(*) FROM sessions WHERE user_id = $1"#,
    user_id,
  )
  .fetch_one(pool)
  .await
  .unwrap()
  .unwrap_or(0)
}

async fn sign_in(
  router: &topcoat::router::Router,
  email: &str,
  password: &str,
) -> common::JsonResponse {
  json_post(
    router,
    "/api/auth/sign-in/email",
    None,
    serde_json::json!({ "email": email, "password": password }),
  )
  .await
}

#[tokio::test]
async fn admin_lists_and_searches_users() {
  let (pool, router) = pool_and_router().await;
  let admin_email = unique_email("list-admin");
  let target_email = unique_email("list-target");

  let admin = sign_up(&router, "List Admin", &admin_email, "password123").await;
  assert_eq!(admin.status, StatusCode::OK);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let target =
    sign_up(&router, "Alvo Busca", &target_email, "password123").await;
  assert_eq!(target.status, StatusCode::OK);

  let listed =
    json_get(&router, "/api/auth/admin/list-users", Some(&admin_cookie)).await;
  assert_eq!(listed.status, StatusCode::OK, "{:?}", listed.json);
  let users = listed.json["users"].as_array().expect("users");
  assert!(
    users.iter().any(|user| user["email"] == target_email),
    "{:?}",
    listed.json
  );
  assert!(listed.json["total"].as_i64().unwrap() >= 2);

  let needle = target_email.split('@').next().unwrap();
  let searched = json_get(
    &router,
    &format!("/api/auth/admin/list-users?q={needle}"),
    Some(&admin_cookie),
  )
  .await;
  assert_eq!(searched.status, StatusCode::OK, "{:?}", searched.json);
  assert_eq!(searched.json["total"].as_i64().unwrap(), 1);
  assert_eq!(searched.json["users"][0]["email"], target_email);
  assert_eq!(searched.json["users"][0]["name"], "Alvo Busca");
}

#[tokio::test]
async fn non_admin_cannot_list_or_ban() {
  let (_pool, router) = pool_and_router().await;
  let user = sign_up(
    &router,
    "Regular",
    &unique_email("list-user"),
    "password123",
  )
  .await;
  assert_eq!(user.status, StatusCode::OK);
  let cookie = user.next_cookie.clone();
  let other =
    sign_up(&router, "Other", &unique_email("list-other"), "password123").await;
  let other_id = other.json["user"]["id"].as_str().unwrap();

  let listed =
    json_get(&router, "/api/auth/admin/list-users", cookie.as_deref()).await;
  assert_eq!(listed.status, StatusCode::FORBIDDEN, "{:?}", listed.json);

  let banned = json_post(
    &router,
    "/api/auth/admin/ban-user",
    cookie.as_deref(),
    serde_json::json!({ "userId": other_id, "banReason": "nope" }),
  )
  .await;
  assert_eq!(banned.status, StatusCode::FORBIDDEN, "{:?}", banned.json);
}

#[tokio::test]
async fn admin_bans_and_unbans_user() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Ban Admin",
    &unique_email("ban-admin"),
    "password123",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let target_email = unique_email("ban-target");
  let target =
    sign_up(&router, "Ban Target", &target_email, "password123").await;
  let target_id = user_id(&target.json);
  assert!(session_count(&pool, target_id).await >= 1);

  let banned = json_post(
    &router,
    "/api/auth/admin/ban-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id, "banReason": "abuso" }),
  )
  .await;
  assert_eq!(banned.status, StatusCode::OK, "{:?}", banned.json);

  let row = sqlx::query!(
    r#"SELECT banned, ban_reason, ban_expires FROM users WHERE id = $1"#,
    target_id,
  )
  .fetch_one(&pool)
  .await
  .unwrap();
  assert!(row.banned);
  assert_eq!(row.ban_reason.as_deref(), Some("abuso"));
  assert!(row.ban_expires.is_none());
  assert_eq!(session_count(&pool, target_id).await, 0);

  let denied = sign_in(&router, &target_email, "password123").await;
  assert_eq!(denied.status, StatusCode::UNAUTHORIZED, "{:?}", denied.json);

  let unbanned = json_post(
    &router,
    "/api/auth/admin/unban-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id }),
  )
  .await;
  assert_eq!(unbanned.status, StatusCode::OK, "{:?}", unbanned.json);

  let row = sqlx::query!(
    r#"SELECT banned, ban_reason, ban_expires FROM users WHERE id = $1"#,
    target_id,
  )
  .fetch_one(&pool)
  .await
  .unwrap();
  assert!(!row.banned);
  assert!(row.ban_reason.is_none());
  assert!(row.ban_expires.is_none());
  assert_eq!(session_count(&pool, target_id).await, 0);

  let allowed = sign_in(&router, &target_email, "password123").await;
  assert_eq!(allowed.status, StatusCode::OK, "{:?}", allowed.json);

  let blank_email = unique_email("ban-blank");
  let blank =
    sign_up(&router, "Blank Reason", &blank_email, "password123").await;
  let blank_id = user_id(&blank.json);
  let blank_ban = json_post(
    &router,
    "/api/auth/admin/ban-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": blank_id, "banReason": "  " }),
  )
  .await;
  assert_eq!(blank_ban.status, StatusCode::OK, "{:?}", blank_ban.json);
  let reason: Option<String> = sqlx::query_scalar!(
    r#"SELECT ban_reason FROM users WHERE id = $1"#,
    blank_id,
  )
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(reason.as_deref(), Some("Sem motivo"));
}

#[tokio::test]
async fn admin_cannot_ban_self() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Self Ban",
    &unique_email("self-ban"),
    "password123",
  )
  .await;
  let admin_id = user_id(&admin.json);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, admin_id, "admin").await;

  let denied = json_post(
    &router,
    "/api/auth/admin/ban-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": admin_id }),
  )
  .await;
  assert_eq!(denied.status, StatusCode::BAD_REQUEST, "{:?}", denied.json);

  let banned: bool =
    sqlx::query_scalar!(r#"SELECT banned FROM users WHERE id = $1"#, admin_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert!(!banned);
}

#[tokio::test]
async fn admin_sets_role_and_name() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Role Admin",
    &unique_email("role-admin"),
    "password123",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let target = sign_up(
    &router,
    "Old Name",
    &unique_email("role-target"),
    "password123",
  )
  .await;
  let target_id = target.json["user"]["id"].as_str().unwrap();

  let role = json_post(
    &router,
    "/api/auth/admin/set-role",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id, "role": "moderator" }),
  )
  .await;
  assert_eq!(role.status, StatusCode::OK, "{:?}", role.json);
  assert_eq!(role.json["user"]["role"], "moderator");

  let renamed = json_post(
    &router,
    "/api/auth/admin/update-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id, "name": "  Novo Nome  " }),
  )
  .await;
  assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.json);
  assert_eq!(renamed.json["user"]["name"], "Novo Nome");
}

#[tokio::test]
async fn admin_removes_user_but_not_self() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Remove Admin",
    &unique_email("remove-admin"),
    "password123",
  )
  .await;
  let admin_id = user_id(&admin.json);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, admin_id, "admin").await;

  let target = sign_up(
    &router,
    "Remove Target",
    &unique_email("remove-target"),
    "password123",
  )
  .await;
  let target_id = user_id(&target.json);

  let removed = json_post(
    &router,
    "/api/auth/admin/remove-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id }),
  )
  .await;
  assert_eq!(removed.status, StatusCode::OK, "{:?}", removed.json);
  let still_there = sqlx::query_scalar!(
    r#"SELECT id AS "id!" FROM users WHERE id = $1"#,
    target_id,
  )
  .fetch_optional(&pool)
  .await
  .unwrap();
  assert!(still_there.is_none());

  let denied = json_post(
    &router,
    "/api/auth/admin/remove-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": admin_id }),
  )
  .await;
  assert_eq!(denied.status, StatusCode::BAD_REQUEST, "{:?}", denied.json);
  let admin_still = sqlx::query_scalar!(
    r#"SELECT id AS "id!" FROM users WHERE id = $1"#,
    admin_id,
  )
  .fetch_optional(&pool)
  .await
  .unwrap();
  assert!(admin_still.is_some());
}

#[tokio::test]
async fn admin_revokes_one_session_and_all_sessions() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Session Admin",
    &unique_email("sess-admin"),
    "password123",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let email = unique_email("sess-target");
  let target = sign_up(&router, "Session Target", &email, "password123").await;
  let target_id = user_id(&target.json);
  assert_eq!(session_count(&pool, target_id).await, 1);

  let listed = json_post(
    &router,
    "/api/auth/admin/list-user-sessions",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id }),
  )
  .await;
  assert_eq!(listed.status, StatusCode::OK, "{:?}", listed.json);
  let session_id = listed.json["sessions"][0]["id"].as_str().unwrap();

  let revoked = json_post(
    &router,
    "/api/auth/admin/revoke-user-session",
    Some(&admin_cookie),
    serde_json::json!({ "sessionId": session_id }),
  )
  .await;
  assert_eq!(revoked.status, StatusCode::OK, "{:?}", revoked.json);
  assert_eq!(session_count(&pool, target_id).await, 0);

  assert_eq!(
    sign_in(&router, &email, "password123").await.status,
    StatusCode::OK
  );
  assert_eq!(
    sign_in(&router, &email, "password123").await.status,
    StatusCode::OK
  );
  assert_eq!(session_count(&pool, target_id).await, 2);

  let revoked_all = json_post(
    &router,
    "/api/auth/admin/revoke-user-sessions",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id }),
  )
  .await;
  assert_eq!(revoked_all.status, StatusCode::OK, "{:?}", revoked_all.json);
  assert_eq!(session_count(&pool, target_id).await, 0);
}

#[tokio::test]
async fn admin_cannot_impersonate_admin() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Imp Admin",
    &unique_email("imp-admin"),
    "password123",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let other = sign_up(
    &router,
    "Other Admin",
    &unique_email("imp-other"),
    "password123",
  )
  .await;
  let other_id = user_id(&other.json);
  promote(&pool, other_id, "admin").await;

  let denied = json_post(
    &router,
    "/api/auth/admin/impersonate-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": other_id }),
  )
  .await;
  assert_eq!(denied.status, StatusCode::FORBIDDEN, "{:?}", denied.json);
}
