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

use crate::auth::routes::dto::random_token;
use crate::auth::service::{
  verification_token_hash, verification_token_hash_prefix,
};
use crate::auth::session_store;
use crate::auth::step_up::{forbid_impersonated, verify_step_up};
use crate::auth::user::current_user;

const DELETE_TOKEN_TTL_SECS: i64 = 30 * 60;
const DELETE_IDENTIFIER_PREFIX: &str = "delete-user:";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteUserBody {
  /// Password re-check (required on the request path when the account
  /// has a credential password).
  pub password: Option<String>,
  /// TOTP code alternative (required when 2FA is enabled and no password is
  /// given, or when the account has no credential password).
  pub code: Option<String>,
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
    .map_err(|e| crate::internal_error::internal_error_cx(cx, &e))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };
  forbid_impersonated(su)?;

  let pool = app_context::<PgPool>(cx);

  // The request path requires a fresh password or TOTP check on top of
  // the session (TOTP with the per-user cap and replay guard). OAuth-only
  // accounts without 2FA have nothing to re-check beyond the session. The
  // token-confirm path above stays token-only (mail flow).
  verify_step_up(
    cx,
    pool,
    su.user.id,
    body.password.as_deref(),
    body.code.as_deref(),
  )
  .await?;

  // Drop prior pending delete tokens for this user.
  let identifier = format!("{DELETE_IDENTIFIER_PREFIX}{}", su.user.id);
  sqlx::query!(
    r#"DELETE FROM verifications WHERE identifier = $1"#,
    identifier,
  )
  .execute(pool)
  .await?;

  let token = random_token();
  let token_hash = verification_token_hash(&token);
  let token_hash_prefix =
    verification_token_hash_prefix(&token_hash).to_owned();
  let id = Uuid::now_v7();
  let expires_at = time::OffsetDateTime::now_utc()
    + time::Duration::seconds(DELETE_TOKEN_TTL_SECS);

  sqlx::query!(
        r#"
        INSERT INTO verifications (id, identifier, value, value_hash, expires_at, created_at, updated_at)
        VALUES ($1, $2, '', $3, $4, now(), now())
        "#,
        id,
        identifier,
        token_hash,
        expires_at,
    )
    .execute(pool)
    .await?;

  crate::mail::auth::send_delete_account(
    pool,
    &su.user.email,
    &su.user.name,
    &token,
  )
  .await?;

  tracing::info!(
      user_id = %su.user.id,
      callback_url = ?body.callback_url,
      token_hash_prefix = %token_hash_prefix,
      "delete-user verification token created; mail queued"
  );

  Ok(Json(OkStatus { status: true }))
}

/// Token-only confirm, shared with the `/excluir-conta` mail landing page.
pub(crate) async fn complete_delete(
  cx: &Cx,
  token: &str,
) -> Result<Json<OkStatus>> {
  let pool = app_context::<PgPool>(cx);

  // Confirm stays token-only (no session) so mail links work logged-out.
  let token_hash = verification_token_hash(token.trim());
  let verification = sqlx::query!(
    r#"
        SELECT id, identifier
        FROM verifications
        WHERE value_hash = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    token_hash,
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

  // FK cascades remove sessions, accounts, two_factor, addresses,
  // wishlist and cart; orders are kept (user_id SET NULL, contact email
  // preserved by the `users_keep_order_contact` trigger, 0027).
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
