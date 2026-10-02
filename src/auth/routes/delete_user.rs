//! Delete user: request verification + confirm (Better Auth-style).

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

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::verify_password;
use crate::auth::routes::dto::random_token;
use crate::auth::session_store;
use crate::auth::user::current_user;

const DELETE_TOKEN_TTL_SECS: i64 = 60 * 60;
const DELETE_IDENTIFIER_PREFIX: &str = "delete-user:";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteUserBody {
  /// Optional password check before sending the delete verification email.
  pub password: Option<String>,
  /// When present, completes deletion instead of requesting verification.
  pub token: Option<String>,
  pub callback_url: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[utoipa::path(
    post,
    path = "/api/auth/delete-user",
    tag = "auth",
    request_body = DeleteUserBody,
    responses(
        (status = 200, description = "Delete verification queued, or account deleted", body = OkStatus)
    )
)]
#[route(POST "/api/auth/delete-user")]
pub async fn delete_user(
  cx: &Cx,
  Json(body): Json<DeleteUserBody>,
) -> Result<Json<OkStatus>> {
  if let Some(token) = body
    .token
    .as_deref()
    .map(str::trim)
    .filter(|t| !t.is_empty())
  {
    return complete_delete(cx, token).await;
  }

  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let pool = app_context::<PgPool>(cx);

  if let Some(password) = body.password.as_deref().filter(|p| !p.is_empty()) {
    let account = sqlx::query!(
      r#"
            SELECT password
            FROM accounts
            WHERE user_id = $1 AND provider_id = $2
            "#,
      su.user.id,
      CREDENTIAL_PROVIDER_ID,
    )
    .fetch_optional(pool)
    .await?;

    let Some(account) = account else {
      return Err(unauthorized().into());
    };
    let Some(hash) = account.password.as_deref() else {
      return Err(unauthorized().into());
    };
    let ok = verify_password(password, hash).map_err(|e| {
      topcoat::Error::from(std::io::Error::other(e.to_string()))
    })?;
    if !ok {
      return Err(unauthorized().into());
    }
  }

  // Drop prior pending delete tokens for this user.
  let identifier = format!("{DELETE_IDENTIFIER_PREFIX}{}", su.user.id);
  sqlx::query!(
    r#"DELETE FROM verifications WHERE identifier = $1"#,
    identifier,
  )
  .execute(pool)
  .await?;

  let token = random_token();
  let id = Uuid::now_v7();
  let expires_at = time::OffsetDateTime::now_utc()
    + time::Duration::seconds(DELETE_TOKEN_TTL_SECS);
  let expires_at =
    time::PrimitiveDateTime::new(expires_at.date(), expires_at.time());

  sqlx::query!(
        r#"
        INSERT INTO verifications (id, identifier, value, expires_at, created_at, updated_at)
        VALUES ($1, $2, $3, $4, now(), now())
        "#,
        id,
        identifier,
        token,
        expires_at,
    )
    .execute(pool)
    .await?;

  let delete_url = format!("/api/auth/delete-user?token={token}");
  tracing::info!(
      user_id = %su.user.id,
      email = %su.user.email,
      callback_url = ?body.callback_url,
      delete_token = %token,
      delete_url = %delete_url,
      "delete-user verification token created (mail not wired yet)"
  );

  Ok(Json(OkStatus { status: true }))
}

async fn complete_delete(cx: &Cx, token: &str) -> Result<Json<OkStatus>> {
  let pool = app_context::<PgPool>(cx);

  let verification = sqlx::query!(
    r#"
        SELECT id, identifier
        FROM verifications
        WHERE value = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    token,
    format!("{DELETE_IDENTIFIER_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?;

  let Some(verification) = verification else {
    return Err(bad_request("invalid or expired token").into());
  };

  let user_id_str = verification
    .identifier
    .strip_prefix(DELETE_IDENTIFIER_PREFIX)
    .unwrap_or(&verification.identifier);
  let Ok(user_id) = Uuid::parse_str(user_id_str) else {
    return Err(bad_request("invalid or expired token").into());
  };

  sqlx::query!("DELETE FROM verifications WHERE id = $1", verification.id)
    .execute(pool)
    .await?;

  // Cascades sessions / accounts / two_factor via FK ON DELETE cascade.
  let deleted = sqlx::query!(r#"DELETE FROM users WHERE id = $1"#, user_id)
    .execute(pool)
    .await?;

  if deleted.rows_affected() == 0 {
    return Err(bad_request("invalid or expired token").into());
  }

  // Clear cookie if this request still has a session.
  if let Some(hash) = session::stop(cx).await? {
    let _ = session_store::delete_by_token_hash(pool, &hash).await;
  }

  Ok(Json(OkStatus { status: true }))
}
