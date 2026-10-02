//! Session management: list / revoke / revoke-other (authenticated).

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Json,
    error::{bad_request, unauthorized},
    route,
  },
  session,
};
use uuid::Uuid;

use crate::auth::routes::dto::format_primitive;
use crate::auth::session_store::{self, token_hash_hex};
use crate::auth::user::current_user;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedSessionJson {
  pub id: String,
  /// Session id (for Better Auth `revokeSession({ token })` — we store a hash, not the raw token).
  pub token: String,
  pub user_agent: Option<String>,
  pub ip_address: Option<String>,
  pub created_at: String,
  pub expires_at: String,
  /// True when this row matches the request's current cookie token hash.
  pub current: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RevokeSessionBody {
  pub session_id: Option<String>,
  /// Alias for `sessionId` (Better Auth client sends `token`).
  pub token: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[utoipa::path(
    get,
    path = "/api/auth/list-sessions",
    tag = "auth",
    responses(
        (status = 200, description = "Sessions for the current user", body = Vec<ListedSessionJson>),
        (status = 401, description = "Missing session")
    )
)]
#[route(GET "/api/auth/list-sessions")]
pub async fn list_sessions(cx: &Cx) -> Result<Json<Vec<ListedSessionJson>>> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let pool = app_context::<PgPool>(cx);
  let current_token =
    session::token_hash(cx).await?.map(|h| token_hash_hex(&h));

  let rows = sqlx::query!(
    r#"
        SELECT id, token, user_agent, ip_address, created_at, expires_at
        FROM sessions
        WHERE user_id = $1
          AND expires_at > now()
        ORDER BY created_at DESC
        "#,
    su.user.id,
  )
  .fetch_all(pool)
  .await?;

  let list = rows
    .into_iter()
    .map(|r| {
      let id = r.id.to_string();
      let current = current_token
        .as_deref()
        .is_some_and(|t| t == r.token.as_str());
      ListedSessionJson {
        id: id.clone(),
        token: id,
        user_agent: r.user_agent,
        ip_address: r.ip_address,
        created_at: format_primitive(r.created_at),
        expires_at: format_primitive(r.expires_at),
        current,
      }
    })
    .collect();

  Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/auth/revoke-session",
    tag = "auth",
    request_body = RevokeSessionBody,
    responses(
        (status = 200, description = "Session revoked", body = OkStatus),
        (status = 401, description = "Missing session")
    )
)]
#[route(POST "/api/auth/revoke-session")]
pub async fn revoke_session(
  cx: &Cx,
  Json(body): Json<RevokeSessionBody>,
) -> Result<Json<OkStatus>> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let raw = body
    .session_id
    .as_deref()
    .or(body.token.as_deref())
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .ok_or_else(|| bad_request("sessionId or token is required"))?;

  let session_id = Uuid::parse_str(raw).map_err(|_| {
    bad_request("sessionId or token must be a valid session id")
  })?;

  let pool = app_context::<PgPool>(cx);
  let deleted =
    session_store::delete_owned_session(pool, su.user.id, session_id).await?;
  if !deleted {
    return Err(bad_request("session not found").into());
  }

  if session_id == su.session_id {
    let _ = session::stop(cx).await?;
  }

  Ok(Json(OkStatus { status: true }))
}

#[utoipa::path(
    post,
    path = "/api/auth/revoke-other-sessions",
    tag = "auth",
    responses(
        (status = 200, description = "Other sessions revoked", body = OkStatus),
        (status = 401, description = "Missing session")
    )
)]
#[route(POST "/api/auth/revoke-other-sessions")]
pub async fn revoke_other_sessions(cx: &Cx) -> Result<Json<OkStatus>> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let Some(hash) = session::token_hash(cx).await? else {
    return Err(unauthorized().into());
  };

  let pool = app_context::<PgPool>(cx);
  let keep = token_hash_hex(&hash);
  session_store::delete_all_for_user_except_token(pool, su.user.id, &keep)
    .await?;

  Ok(Json(OkStatus { status: true }))
}
