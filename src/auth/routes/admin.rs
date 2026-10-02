//! Admin user management (Better Auth admin plugin-compatible).

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Json,
    error::{bad_request, forbidden, unauthorized},
    query_params, route,
  },
};
use uuid::Uuid;

use crate::auth::routes::dto::{AuthUserJson, SessionPayload};
use crate::auth::service::{self, AdminUserList};
use crate::auth::user::{SessionUser, current_user};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImpersonateUserBody {
  pub user_id: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetRoleBody {
  pub user_id: String,
  pub role: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminUpdateUserBody {
  pub user_id: String,
  pub name: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BanUserBody {
  pub user_id: String,
  #[serde(default)]
  pub ban_reason: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserIdBody {
  pub user_id: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RevokeUserSessionBody {
  pub session_id: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListUsersResponse {
  pub users: Vec<AuthUserJson>,
  pub total: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserResponse {
  pub user: AuthUserJson,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminSessionJson {
  pub id: String,
  pub created_at: String,
  pub expires_at: String,
  pub ip_address: Option<String>,
  pub user_agent: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListUserSessionsResponse {
  pub sessions: Vec<AdminSessionJson>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminOkStatus {
  pub status: bool,
}

#[derive(Debug)]
#[query_params(error = bad_request)]
struct ListUsersQuery {
  q: Option<String>,
  limit: Option<String>,
  offset: Option<String>,
}

async fn admin_actor(cx: &Cx) -> Result<SessionUser> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };
  if !service::is_admin(su.user.role.as_deref()) {
    return Err(forbidden().into());
  }
  Ok(su.clone())
}

fn parse_user_id(raw: &str) -> Result<Uuid> {
  Uuid::parse_str(raw.trim())
    .map_err(|_| bad_request("userId must be a valid uuid").into())
}

fn parse_session_id(raw: &str) -> Result<Uuid> {
  Uuid::parse_str(raw.trim())
    .map_err(|_| bad_request("sessionId must be a valid uuid").into())
}

fn parse_i64(raw: Option<&str>, default: i64) -> Result<i64> {
  match raw.map(str::trim).filter(|s| !s.is_empty()) {
    None => Ok(default),
    Some(value) => value
      .parse::<i64>()
      .map_err(|_| bad_request("limit and offset must be integers").into()),
  }
}

fn list_response(list: AdminUserList) -> ListUsersResponse {
  ListUsersResponse {
    users: list.users.iter().map(AuthUserJson::from).collect(),
    total: list.total,
  }
}

fn ok_status() -> Json<AdminOkStatus> {
  Json(AdminOkStatus { status: true })
}

#[utoipa::path(
    get,
    path = "/api/auth/admin/list-users",
    tag = "auth",
    params(
        ("q" = Option<String>, Query, description = "Match name or email"),
        ("limit" = Option<i64>, Query, description = "Page size, 1 to 100"),
        ("offset" = Option<i64>, Query, description = "Zero-based offset")
    ),
    responses(
        (status = 200, description = "Matching users", body = ListUsersResponse),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(GET "/api/auth/admin/list-users")]
pub async fn list_users(cx: &Cx) -> Result<Json<ListUsersResponse>> {
  let actor = admin_actor(cx).await?;
  let query = query_params::<ListUsersQuery>(cx)?;
  let limit = parse_i64(query.limit.as_deref(), service::ADMIN_USER_PAGE_SIZE)?;
  let offset = parse_i64(query.offset.as_deref(), 0)?;
  let pool = app_context::<PgPool>(cx);
  let list =
    service::list_admin_users(pool, &actor, query.q.as_deref(), limit, offset)
      .await?;
  Ok(Json(list_response(list)))
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/set-role",
    tag = "auth",
    request_body = SetRoleBody,
    responses(
        (status = 200, description = "Role updated", body = AdminUserResponse),
        (status = 400, description = "Invalid role or user"),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/set-role")]
pub async fn set_role(
  cx: &Cx,
  Json(body): Json<SetRoleBody>,
) -> Result<Json<AdminUserResponse>> {
  let actor = admin_actor(cx).await?;
  let user_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  let user = service::set_user_role(pool, &actor, user_id, &body.role).await?;
  Ok(Json(AdminUserResponse {
    user: AuthUserJson::from(&user),
  }))
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/update-user",
    tag = "auth",
    request_body = AdminUpdateUserBody,
    responses(
        (status = 200, description = "Name updated", body = AdminUserResponse),
        (status = 400, description = "Invalid name or user"),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/update-user")]
pub async fn update_user(
  cx: &Cx,
  Json(body): Json<AdminUpdateUserBody>,
) -> Result<Json<AdminUserResponse>> {
  let actor = admin_actor(cx).await?;
  let user_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  let user =
    service::admin_update_user_name(pool, &actor, user_id, &body.name).await?;
  Ok(Json(AdminUserResponse {
    user: AuthUserJson::from(&user),
  }))
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/ban-user",
    tag = "auth",
    request_body = BanUserBody,
    responses(
        (status = 200, description = "User banned", body = AdminOkStatus),
        (status = 400, description = "Cannot ban yourself or user missing"),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/ban-user")]
pub async fn ban_user(
  cx: &Cx,
  Json(body): Json<BanUserBody>,
) -> Result<Json<AdminOkStatus>> {
  let actor = admin_actor(cx).await?;
  let user_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  service::ban_user(pool, &actor, user_id, body.ban_reason.as_deref()).await?;
  Ok(ok_status())
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/unban-user",
    tag = "auth",
    request_body = UserIdBody,
    responses(
        (status = 200, description = "User unbanned", body = AdminOkStatus),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/unban-user")]
pub async fn unban_user(
  cx: &Cx,
  Json(body): Json<UserIdBody>,
) -> Result<Json<AdminOkStatus>> {
  let actor = admin_actor(cx).await?;
  let user_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  service::unban_user(pool, &actor, user_id).await?;
  Ok(ok_status())
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/remove-user",
    tag = "auth",
    request_body = UserIdBody,
    responses(
        (status = 200, description = "User removed", body = AdminOkStatus),
        (status = 400, description = "Cannot remove yourself"),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/remove-user")]
pub async fn remove_user(
  cx: &Cx,
  Json(body): Json<UserIdBody>,
) -> Result<Json<AdminOkStatus>> {
  let actor = admin_actor(cx).await?;
  let user_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  service::remove_user(pool, &actor, user_id).await?;
  Ok(ok_status())
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/list-user-sessions",
    tag = "auth",
    request_body = UserIdBody,
    responses(
        (status = 200, description = "Active sessions", body = ListUserSessionsResponse),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/list-user-sessions")]
pub async fn list_user_sessions(
  cx: &Cx,
  Json(body): Json<UserIdBody>,
) -> Result<Json<ListUserSessionsResponse>> {
  let actor = admin_actor(cx).await?;
  let user_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  let sessions =
    service::list_admin_user_sessions(pool, &actor, user_id).await?;
  Ok(Json(ListUserSessionsResponse {
    sessions: sessions
      .into_iter()
      .map(|row| AdminSessionJson {
        id: row.id.to_string(),
        created_at: row.created_at,
        expires_at: row.expires_at,
        ip_address: row.ip_address,
        user_agent: row.user_agent,
      })
      .collect(),
  }))
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/revoke-user-session",
    tag = "auth",
    request_body = RevokeUserSessionBody,
    responses(
        (status = 200, description = "Session revoked", body = AdminOkStatus),
        (status = 400, description = "Session not found"),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/revoke-user-session")]
pub async fn revoke_user_session(
  cx: &Cx,
  Json(body): Json<RevokeUserSessionBody>,
) -> Result<Json<AdminOkStatus>> {
  let actor = admin_actor(cx).await?;
  let session_id = parse_session_id(&body.session_id)?;
  let pool = app_context::<PgPool>(cx);
  service::revoke_admin_session(pool, &actor, session_id).await?;
  Ok(ok_status())
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/revoke-user-sessions",
    tag = "auth",
    request_body = UserIdBody,
    responses(
        (status = 200, description = "Sessions revoked", body = AdminOkStatus),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin")
    )
)]
#[route(POST "/api/auth/admin/revoke-user-sessions")]
pub async fn revoke_user_sessions(
  cx: &Cx,
  Json(body): Json<UserIdBody>,
) -> Result<Json<AdminOkStatus>> {
  let actor = admin_actor(cx).await?;
  let user_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  service::revoke_admin_user_sessions(pool, &actor, user_id).await?;
  Ok(ok_status())
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/impersonate-user",
    tag = "auth",
    request_body = ImpersonateUserBody,
    responses(
        (status = 200, description = "Now impersonating target user", body = SessionPayload),
        (status = 401, description = "Missing session"),
        (status = 403, description = "Not an admin, or target is an admin")
    )
)]
#[route(POST "/api/auth/admin/impersonate-user")]
pub async fn impersonate_user(
  cx: &Cx,
  Json(body): Json<ImpersonateUserBody>,
) -> Result<Json<SessionPayload>> {
  let actor = admin_actor(cx).await?;
  let target_id = parse_user_id(&body.user_id)?;
  let pool = app_context::<PgPool>(cx);
  let impersonated =
    service::impersonate_user(cx, pool, &actor, target_id).await?;
  Ok(Json(SessionPayload::from(&impersonated)))
}

#[utoipa::path(
    post,
    path = "/api/auth/admin/stop-impersonating",
    tag = "auth",
    responses(
        (status = 200, description = "Restored admin session", body = SessionPayload),
        (status = 400, description = "Not currently impersonating"),
        (status = 401, description = "Missing session")
    )
)]
#[route(POST "/api/auth/admin/stop-impersonating")]
pub async fn stop_impersonating(cx: &Cx) -> Result<Json<SessionPayload>> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let pool = app_context::<PgPool>(cx);
  let restored = service::stop_impersonating(cx, pool, su).await?;
  Ok(Json(SessionPayload::from(&restored)))
}
