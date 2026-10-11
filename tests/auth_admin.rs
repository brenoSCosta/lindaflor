use http::StatusCode;
use test_support::{
  json_get, json_post, pool_and_router, sign_up, unique_email,
};
use uuid::Uuid;

#[tokio::test]
async fn admin_impersonate_and_stop() {
  let (pool, router) = pool_and_router().await;

  let admin_email = unique_email("admin");
  let target_email = unique_email("target");

  let admin = sign_up(
    &router,
    "Admin User",
    &admin_email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(admin.status, StatusCode::OK);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  let admin_id =
    Uuid::parse_str(admin.json["user"]["id"].as_str().unwrap()).unwrap();

  sqlx::query!(r#"UPDATE users SET role = 'admin' WHERE id = $1"#, admin_id)
    .execute(&pool)
    .await
    .unwrap();

  let target = sign_up(
    &router,
    "Target User",
    &target_email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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

  let user = sign_up(
    &router,
    "Regular",
    &unique_email("nonadmin"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(user.status, StatusCode::OK);
  let cookie = user.next_cookie.clone();

  let other = sign_up(
    &router,
    "Other",
    &unique_email("other"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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
) -> test_support::JsonResponse {
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

  let admin = sign_up(
    &router,
    "List Admin",
    &admin_email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(admin.status, StatusCode::OK);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let target = sign_up(
    &router,
    "Alvo Busca",
    &target_email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  assert_eq!(user.status, StatusCode::OK);
  let cookie = user.next_cookie.clone();
  let other = sign_up(
    &router,
    "Other",
    &unique_email("list-other"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let target_email = unique_email("ban-target");
  let target = sign_up(
    &router,
    "Ban Target",
    &target_email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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

  let denied =
    sign_in(&router, &target_email, "Harbor-Violet-Tiger-4821-Lantern!").await;
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

  let allowed =
    sign_in(&router, &target_email, "Harbor-Violet-Tiger-4821-Lantern!").await;
  assert_eq!(allowed.status, StatusCode::OK, "{:?}", allowed.json);

  let blank_email = unique_email("ban-blank");
  let blank = sign_up(
    &router,
    "Blank Reason",
    &blank_email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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
    "Harbor-Violet-Tiger-4821-Lantern!",
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
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let target = sign_up(
    &router,
    "Old Name",
    &unique_email("role-target"),
    "Harbor-Violet-Tiger-4821-Lantern!",
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
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_id = user_id(&admin.json);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, admin_id, "admin").await;

  let target = sign_up(
    &router,
    "Remove Target",
    &unique_email("remove-target"),
    "Harbor-Violet-Tiger-4821-Lantern!",
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

  // Audit row survives the delete: entity_id has no FK and keeps the
  // removed id; actor snapshot is preserved.
  let (
    agg_type,
    agg_id,
    entity_type,
    entity_id,
    event_actor,
    event_actor_name,
    event_metadata,
  ) = audit_row(&pool, admin_id, &target_id.to_string(), "remove_user").await;
  assert_eq!(agg_type, "user");
  assert_eq!(agg_id, target_id.to_string());
  assert_eq!(entity_type, "user");
  assert_eq!(entity_id, target_id);
  assert_eq!(event_actor, Some(admin_id));
  assert_eq!(event_actor_name.as_deref(), Some("Remove Admin"));
  assert_eq!(
    event_metadata.get("action").and_then(|v| v.as_str()),
    Some("remove_user")
  );

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
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let email = unique_email("sess-target");
  let target = sign_up(
    &router,
    "Session Target",
    &email,
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
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
    serde_json::json!({ "sessionId": session_id, "userId": target_id }),
  )
  .await;
  assert_eq!(revoked.status, StatusCode::OK, "{:?}", revoked.json);
  assert_eq!(session_count(&pool, target_id).await, 0);

  assert_eq!(
    sign_in(&router, &email, "Harbor-Violet-Tiger-4821-Lantern!")
      .await
      .status,
    StatusCode::OK
  );
  assert_eq!(
    sign_in(&router, &email, "Harbor-Violet-Tiger-4821-Lantern!")
      .await
      .status,
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
  let admin_id = user_id(&admin.json);
  assert_eq!(
    audit_count(
      &pool,
      admin_id,
      &target_id.to_string(),
      "revoke_all_sessions"
    )
    .await,
    1
  );
  let (
    agg_type,
    agg_id,
    entity_type,
    entity_id,
    event_actor,
    _,
    event_metadata,
  ) = audit_row(
    &pool,
    admin_id,
    &target_id.to_string(),
    "revoke_all_sessions",
  )
  .await;
  assert_eq!(agg_type, "user");
  assert_eq!(agg_id, target_id.to_string());
  assert_eq!(entity_type, "user");
  assert_eq!(entity_id, target_id);
  assert_eq!(event_actor, Some(admin_id));
  assert_eq!(
    event_metadata.get("action").and_then(|v| v.as_str()),
    Some("revoke_all_sessions")
  );
}

async fn audit_count(
  pool: &sqlx::PgPool,
  actor: Uuid,
  aggregate_id: &str,
  action: &str,
) -> i64 {
  sqlx::query_scalar!(
    r#"SELECT COUNT(*) FROM audit_events WHERE actor_user_id = $1 AND aggregate_type = 'user' AND aggregate_id = $2 AND metadata->>'action' = $3"#,
    actor,
    aggregate_id,
    action,
  )
  .fetch_one(pool)
  .await
  .unwrap()
  .unwrap_or(0)
}

async fn audit_row(
  pool: &sqlx::PgPool,
  actor: Uuid,
  aggregate_id: &str,
  action: &str,
) -> (
  String,
  String,
  String,
  Uuid,
  Option<Uuid>,
  Option<String>,
  serde_json::Value,
) {
  let row = sqlx::query!(
    r#"SELECT aggregate_type, aggregate_id, entity_type, entity_id AS "entity_id!", actor_user_id, actor_name, metadata::text AS "metadata!" FROM audit_events WHERE actor_user_id = $1 AND aggregate_type = 'user' AND aggregate_id = $2 AND metadata->>'action' = $3 ORDER BY occurred_at DESC LIMIT 1"#,
    actor,
    aggregate_id,
    action,
  )
  .fetch_one(pool)
  .await
  .unwrap();
  let metadata: serde_json::Value =
    serde_json::from_str(&row.metadata).expect("metadata json");
  (
    row.aggregate_type,
    row.aggregate_id,
    row.entity_type,
    row.entity_id,
    row.actor_user_id,
    row.actor_name,
    metadata,
  )
}

#[tokio::test]
async fn admin_cannot_impersonate_admin() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Imp Admin",
    &unique_email("imp-admin"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, user_id(&admin.json), "admin").await;

  let other = sign_up(
    &router,
    "Other Admin",
    &unique_email("imp-other"),
    "Harbor-Violet-Tiger-4821-Lantern!",
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

#[tokio::test]
async fn admin_cannot_self_demote_via_set_role() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Self Role",
    &unique_email("self-role"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_id = user_id(&admin.json);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, admin_id, "admin").await;

  let denied = json_post(
    &router,
    "/api/auth/admin/set-role",
    Some(&admin_cookie),
    serde_json::json!({ "userId": admin_id, "role": "user" }),
  )
  .await;
  assert_eq!(denied.status, StatusCode::BAD_REQUEST, "{:?}", denied.json);

  let role: Option<String> =
    sqlx::query_scalar!(r#"SELECT role FROM users WHERE id = $1"#, admin_id,)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(role.as_deref(), Some("admin"));
  assert_eq!(
    audit_count(&pool, admin_id, &admin_id.to_string(), "set_role").await,
    0
  );
}

#[tokio::test]
async fn admin_cannot_target_admin_users() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Guard Admin",
    &unique_email("guard-admin"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_id = user_id(&admin.json);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, admin_id, "admin").await;

  let other = sign_up(
    &router,
    "Guard Other",
    &unique_email("guard-other"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let other_id = user_id(&other.json);
  promote(&pool, other_id, "admin").await;

  for (path, body) in [
    (
      "/api/auth/admin/set-role",
      serde_json::json!({ "userId": other_id, "role": "user" }),
    ),
    (
      "/api/auth/admin/ban-user",
      serde_json::json!({ "userId": other_id }),
    ),
    (
      "/api/auth/admin/unban-user",
      serde_json::json!({ "userId": other_id }),
    ),
    (
      "/api/auth/admin/remove-user",
      serde_json::json!({ "userId": other_id }),
    ),
    (
      "/api/auth/admin/revoke-user-sessions",
      serde_json::json!({ "userId": other_id }),
    ),
  ] {
    let denied = json_post(&router, path, Some(&admin_cookie), body).await;
    assert_eq!(
      denied.status,
      StatusCode::FORBIDDEN,
      "{path}: {:?}",
      denied.json
    );
  }

  // Other admin untouched.
  let still_admin: Option<String> =
    sqlx::query_scalar!(r#"SELECT role FROM users WHERE id = $1"#, other_id,)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(still_admin.as_deref(), Some("admin"));
}

#[tokio::test]
async fn admin_revoke_session_enforces_ownership() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Revoke Admin",
    &unique_email("revoke-admin"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_id = user_id(&admin.json);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, admin_id, "admin").await;

  let second_admin = sign_up(
    &router,
    "Second Admin",
    &unique_email("revoke-admin2"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let second_admin_id = user_id(&second_admin.json);
  promote(&pool, second_admin_id, "admin").await;

  let target_a = sign_up(
    &router,
    "Target A",
    &unique_email("revoke-a"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let target_a_id = user_id(&target_a.json);
  let target_b = sign_up(
    &router,
    "Target B",
    &unique_email("revoke-b"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let target_b_id = user_id(&target_b.json);

  let listed = json_post(
    &router,
    "/api/auth/admin/list-user-sessions",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_a_id }),
  )
  .await;
  assert_eq!(listed.status, StatusCode::OK, "{:?}", listed.json);
  let session_id = listed.json["sessions"][0]["id"].as_str().unwrap();

  // Mismatched user_id claim is rejected and the session survives.
  let mismatched = json_post(
    &router,
    "/api/auth/admin/revoke-user-session",
    Some(&admin_cookie),
    serde_json::json!({ "sessionId": session_id, "userId": target_b_id }),
  )
  .await;
  assert_eq!(
    mismatched.status,
    StatusCode::FORBIDDEN,
    "{:?}",
    mismatched.json
  );
  assert_eq!(session_count(&pool, target_a_id).await, 1);

  // Admin's own session cannot be revoked through the admin endpoint.
  let admin_listed = json_post(
    &router,
    "/api/auth/admin/list-user-sessions",
    Some(&admin_cookie),
    serde_json::json!({ "userId": admin_id }),
  )
  .await;
  assert_eq!(
    admin_listed.status,
    StatusCode::OK,
    "{:?}",
    admin_listed.json
  );
  let own_session = admin_listed.json["sessions"][0]["id"].as_str().unwrap();
  let own_denied = json_post(
    &router,
    "/api/auth/admin/revoke-user-session",
    Some(&admin_cookie),
    serde_json::json!({ "sessionId": own_session, "userId": admin_id }),
  )
  .await;
  assert_eq!(
    own_denied.status,
    StatusCode::BAD_REQUEST,
    "{:?}",
    own_denied.json
  );

  // Another admin's session is off limits.
  let other_listed = json_post(
    &router,
    "/api/auth/admin/list-user-sessions",
    Some(&admin_cookie),
    serde_json::json!({ "userId": second_admin_id }),
  )
  .await;
  assert_eq!(
    other_listed.status,
    StatusCode::OK,
    "{:?}",
    other_listed.json
  );
  let other_session = other_listed.json["sessions"][0]["id"].as_str().unwrap();
  let other_denied = json_post(
    &router,
    "/api/auth/admin/revoke-user-session",
    Some(&admin_cookie),
    serde_json::json!({ "sessionId": other_session, "userId": second_admin_id }),
  )
  .await;
  assert_eq!(
    other_denied.status,
    StatusCode::FORBIDDEN,
    "{:?}",
    other_denied.json
  );

  // Correct ownership revokes and writes audit.
  let revoked = json_post(
    &router,
    "/api/auth/admin/revoke-user-session",
    Some(&admin_cookie),
    serde_json::json!({ "sessionId": session_id, "userId": target_a_id }),
  )
  .await;
  assert_eq!(revoked.status, StatusCode::OK, "{:?}", revoked.json);
  assert_eq!(session_count(&pool, target_a_id).await, 0);
  assert_eq!(
    audit_count(&pool, admin_id, &target_a_id.to_string(), "revoke_session")
      .await,
    1
  );
  let (
    agg_type,
    agg_id,
    entity_type,
    entity_id,
    event_actor,
    event_actor_name,
    event_metadata,
  ) = audit_row(&pool, admin_id, &target_a_id.to_string(), "revoke_session")
    .await;
  assert_eq!(agg_type, "user");
  assert_eq!(agg_id, target_a_id.to_string());
  assert_eq!(entity_type, "session");
  assert_eq!(
    entity_id,
    Uuid::parse_str(session_id).expect("session uuid")
  );
  assert_eq!(event_actor, Some(admin_id));
  assert_eq!(event_actor_name.as_deref(), Some("Revoke Admin"));
  assert_eq!(
    event_metadata.get("action").and_then(|v| v.as_str()),
    Some("revoke_session")
  );
  assert_eq!(
    event_metadata.get("session_id").and_then(|v| v.as_str()),
    Some(session_id)
  );

  // userId is required (no backwards-compat optional): omitting it is 400.
  let missing_user = json_post(
    &router,
    "/api/auth/admin/revoke-user-session",
    Some(&admin_cookie),
    serde_json::json!({ "sessionId": session_id }),
  )
  .await;
  assert_eq!(
    missing_user.status,
    StatusCode::BAD_REQUEST,
    "{:?}",
    missing_user.json
  );
}

#[tokio::test]
async fn admin_mutations_write_audit_log() {
  let (pool, router) = pool_and_router().await;
  let admin = sign_up(
    &router,
    "Audit Admin",
    &unique_email("audit-admin"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let admin_id = user_id(&admin.json);
  let admin_cookie = admin.next_cookie.clone().expect("admin cookie");
  promote(&pool, admin_id, "admin").await;

  let target = sign_up(
    &router,
    "Audit Target",
    &unique_email("audit-target"),
    "Harbor-Violet-Tiger-4821-Lantern!",
  )
  .await;
  let target_id = user_id(&target.json);

  let role = json_post(
    &router,
    "/api/auth/admin/set-role",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id, "role": "moderator" }),
  )
  .await;
  assert_eq!(role.status, StatusCode::OK, "{:?}", role.json);

  let banned = json_post(
    &router,
    "/api/auth/admin/ban-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id, "banReason": "audit" }),
  )
  .await;
  assert_eq!(banned.status, StatusCode::OK, "{:?}", banned.json);

  let unbanned = json_post(
    &router,
    "/api/auth/admin/unban-user",
    Some(&admin_cookie),
    serde_json::json!({ "userId": target_id }),
  )
  .await;
  assert_eq!(unbanned.status, StatusCode::OK, "{:?}", unbanned.json);

  assert_eq!(
    audit_count(&pool, admin_id, &target_id.to_string(), "set_role").await,
    1
  );
  assert_eq!(
    audit_count(&pool, admin_id, &target_id.to_string(), "ban_user").await,
    1
  );
  assert_eq!(
    audit_count(&pool, admin_id, &target_id.to_string(), "unban_user").await,
    1
  );

  let (
    agg_type,
    agg_id,
    entity_type,
    entity_id,
    event_actor,
    event_actor_name,
    event_metadata,
  ) = audit_row(&pool, admin_id, &target_id.to_string(), "ban_user").await;
  assert_eq!(agg_type, "user");
  assert_eq!(agg_id, target_id.to_string());
  assert_eq!(entity_type, "user");
  assert_eq!(entity_id, target_id);
  assert_eq!(event_actor, Some(admin_id));
  assert_eq!(event_actor_name.as_deref(), Some("Audit Admin"));
  assert_eq!(
    event_metadata.get("action").and_then(|v| v.as_str()),
    Some("ban_user")
  );
  assert_eq!(
    event_metadata.get("reason").and_then(|v| v.as_str()),
    Some("audit")
  );

  let (_, _, _, _, _, _, role_metadata) =
    audit_row(&pool, admin_id, &target_id.to_string(), "set_role").await;
  assert_eq!(
    role_metadata.get("action").and_then(|v| v.as_str()),
    Some("set_role")
  );
  assert_eq!(
    role_metadata.get("role").and_then(|v| v.as_str()),
    Some("moderator")
  );
}
