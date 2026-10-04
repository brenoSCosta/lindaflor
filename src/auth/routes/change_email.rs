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
};
use uuid::Uuid;

use crate::auth::routes::dto::{normalize_email, random_token};
use crate::auth::user::current_user;

const CHANGE_EMAIL_TOKEN_TTL_SECS: i64 = 60 * 60;
const CHANGE_EMAIL_IDENTIFIER_PREFIX: &str = "change-email:";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEmailBody {
  /// New email address (`newEmail` or `new_email`).
  #[serde(alias = "new_email")]
  pub new_email: String,
  pub callback_url: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmChangeEmailBody {
  pub token: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[utoipa::path(
    post,
    path = "/api/auth/change-email",
    tag = "auth",
    request_body = ChangeEmailBody,
    responses(
        (status = 200, description = "Confirmation email queued (logged in dev)", body = OkStatus)
    )
)]
#[route(POST "/api/auth/change-email")]
pub async fn change_email(
  cx: &Cx,
  Json(body): Json<ChangeEmailBody>,
) -> Result<Json<OkStatus>> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let new_email = normalize_email(&body.new_email);
  if new_email.is_empty() || !new_email.contains('@') {
    return Err(bad_request("newEmail is required").into());
  }
  if new_email == su.user.email {
    return Err(bad_request("newEmail must differ from current email").into());
  }

  let pool = app_context::<PgPool>(cx);

  let taken = sqlx::query!(
    r#"SELECT id FROM users WHERE email = $1 AND id <> $2"#,
    new_email,
    su.user.id,
  )
  .fetch_optional(pool)
  .await?;

  if taken.is_some() {
    return Err(bad_request("email is already in use").into());
  }

  // Drop prior pending change-email tokens for this user.
  let identifier_like =
    format!("{CHANGE_EMAIL_IDENTIFIER_PREFIX}{}:%", su.user.id);
  sqlx::query!(
    r#"DELETE FROM verifications WHERE identifier LIKE $1"#,
    identifier_like,
  )
  .execute(pool)
  .await?;

  let token = random_token();
  let id = Uuid::now_v7();
  // identifier: change-email:{userId}:{newEmail} — value holds the token.
  let identifier =
    format!("{CHANGE_EMAIL_IDENTIFIER_PREFIX}{}:{new_email}", su.user.id);
  let expires_at = time::OffsetDateTime::now_utc()
    + time::Duration::seconds(CHANGE_EMAIL_TOKEN_TTL_SECS);
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

  let confirm_url = format!("/api/auth/change-email/confirm?token={token}");
  tracing::info!(
      user_id = %su.user.id,
      current_email = %su.user.email,
      new_email = %new_email,
      callback_url = ?body.callback_url,
      change_email_token = %token,
      confirm_url = %confirm_url,
      "change-email confirmation token created (mail not wired yet)"
  );

  Ok(Json(OkStatus { status: true }))
}

#[utoipa::path(
    post,
    path = "/api/auth/change-email/confirm",
    tag = "auth",
    request_body = ConfirmChangeEmailBody,
    responses(
        (status = 200, description = "Email updated", body = OkStatus)
    )
)]
#[route(POST "/api/auth/change-email/confirm")]
pub async fn confirm_change_email(
  cx: &Cx,
  Json(body): Json<ConfirmChangeEmailBody>,
) -> Result<Json<OkStatus>> {
  if body.token.trim().is_empty() {
    return Err(bad_request("token is required").into());
  }

  let pool = app_context::<PgPool>(cx);

  let verification = sqlx::query!(
    r#"
        SELECT id, identifier
        FROM verifications
        WHERE value = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    body.token,
    format!("{CHANGE_EMAIL_IDENTIFIER_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?;

  let Some(verification) = verification else {
    return Err(bad_request("invalid or expired token").into());
  };

  let rest = verification
    .identifier
    .strip_prefix(CHANGE_EMAIL_IDENTIFIER_PREFIX)
    .unwrap_or(&verification.identifier);
  let Some((user_id_str, new_email_raw)) = rest.split_once(':') else {
    return Err(bad_request("invalid or expired token").into());
  };
  let Ok(user_id) = Uuid::parse_str(user_id_str) else {
    return Err(bad_request("invalid or expired token").into());
  };
  let new_email = normalize_email(new_email_raw);
  if new_email.is_empty() {
    return Err(bad_request("invalid or expired token").into());
  }

  let taken = sqlx::query!(
    r#"SELECT id FROM users WHERE email = $1 AND id <> $2"#,
    new_email,
    user_id,
  )
  .fetch_optional(pool)
  .await?;

  if taken.is_some() {
    sqlx::query!("DELETE FROM verifications WHERE id = $1", verification.id)
      .execute(pool)
      .await?;
    return Err(bad_request("email is already in use").into());
  }

  let updated = sqlx::query!(
    r#"
        UPDATE users
        SET email = $1, email_verified = true, updated_at = now()
        WHERE id = $2
        "#,
    new_email,
    user_id,
  )
  .execute(pool)
  .await?;

  if updated.rows_affected() == 0 {
    return Err(bad_request("invalid or expired token").into());
  }

  sqlx::query!("DELETE FROM verifications WHERE id = $1", verification.id)
    .execute(pool)
    .await?;

  Ok(Json(OkStatus { status: true }))
}
