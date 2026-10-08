use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{content::Json, error::bad_request, route},
};
use uuid::Uuid;

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::hash_password;
use crate::auth::routes::dto::{normalize_email, random_token};
use crate::auth::service::{
  verification_token_hash, verification_token_hash_prefix,
};
use crate::auth::session_store;

const MIN_PASSWORD_LEN: usize = 8;
const RESET_TOKEN_TTL_SECS: i64 = 60 * 60;
const RESET_IDENTIFIER_PREFIX: &str = "reset-password:";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestPasswordResetBody {
  pub email: String,
  pub redirect_to: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordBody {
  pub token: String,
  pub new_password: String,
}

#[utoipa::path(
    post,
    path = "/api/auth/request-password-reset",
    tag = "auth",
    request_body = RequestPasswordResetBody,
    responses(
        (status = 200, description = "Always succeeds (anti-enumeration)", body = OkStatus)
    )
)]
#[route(POST "/api/auth/request-password-reset")]
pub async fn request_password_reset(
  cx: &Cx,
  Json(body): Json<RequestPasswordResetBody>,
) -> Result<Json<OkStatus>> {
  let email = normalize_email(&body.email);
  let pool = app_context::<PgPool>(cx);

  if !email.is_empty() {
    let user =
      sqlx::query!(r#"SELECT id, name FROM users WHERE email = $1"#, email)
        .fetch_optional(pool)
        .await?;

    if let Some(user) = user {
      let token = random_token();
      let token_hash = verification_token_hash(&token);
      let token_hash_prefix =
        verification_token_hash_prefix(&token_hash).to_owned();
      let id = Uuid::now_v7();
      let identifier = format!("{RESET_IDENTIFIER_PREFIX}{email}");
      let expires_at = time::OffsetDateTime::now_utc()
        + time::Duration::seconds(RESET_TOKEN_TTL_SECS);
      let expires_at =
        time::PrimitiveDateTime::new(expires_at.date(), expires_at.time());

      // Invalidate outstanding reset tokens so only the newest works.
      sqlx::query!(
        r#"DELETE FROM verifications WHERE identifier = $1"#,
        identifier,
      )
      .execute(pool)
      .await?;

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

      tracing::info!(
          user_id = %user.id,
          email = %email,
          redirect_to = ?body.redirect_to,
          token_hash_prefix = %token_hash_prefix,
          "password reset token created (mail not wired yet)"
      );
    }
  }

  Ok(Json(OkStatus { status: true }))
}

#[utoipa::path(
    post,
    path = "/api/auth/reset-password",
    tag = "auth",
    request_body = ResetPasswordBody,
    responses(
        (status = 200, description = "Password updated", body = OkStatus)
    )
)]
#[route(POST "/api/auth/reset-password")]
pub async fn reset_password(
  cx: &Cx,
  Json(body): Json<ResetPasswordBody>,
) -> Result<Json<OkStatus>> {
  if body.token.trim().is_empty() {
    return Err(bad_request("token is required").into());
  }
  if body.new_password.len() < MIN_PASSWORD_LEN {
    return Err(
      bad_request(format!(
        "password must be at least {MIN_PASSWORD_LEN} characters"
      ))
      .into(),
    );
  }

  let pool = app_context::<PgPool>(cx);

  let token_hash = verification_token_hash(body.token.trim());
  let verification = sqlx::query!(
    r#"
        SELECT id, identifier
        FROM verifications
        WHERE value_hash = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    token_hash,
    format!("{RESET_IDENTIFIER_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?;

  let Some(verification) = verification else {
    return Err(bad_request("invalid or expired token").into());
  };

  let email = verification
    .identifier
    .strip_prefix(RESET_IDENTIFIER_PREFIX)
    .unwrap_or(&verification.identifier);
  let email = normalize_email(email);

  let user = sqlx::query!(r#"SELECT id FROM users WHERE email = $1"#, email)
    .fetch_optional(pool)
    .await?;

  let Some(user) = user else {
    return Err(bad_request("invalid or expired token").into());
  };

  let password_hash = hash_password(&body.new_password)?;

  let updated = sqlx::query!(
    r#"
        UPDATE accounts
        SET password = $1, updated_at = now()
        WHERE user_id = $2 AND provider_id = $3
        "#,
    password_hash,
    user.id,
    CREDENTIAL_PROVIDER_ID,
  )
  .execute(pool)
  .await?;

  if updated.rows_affected() == 0 {
    return Err(bad_request("no credential account for user").into());
  }

  // Single-use: consume every outstanding reset token for this email.
  sqlx::query!(
    "DELETE FROM verifications WHERE identifier = $1",
    verification.identifier
  )
  .execute(pool)
  .await?;

  // Revoke all sessions on password reset.
  session_store::delete_all_for_user(pool, user.id).await?;

  Ok(Json(OkStatus { status: true }))
}
