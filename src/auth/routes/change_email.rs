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

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::verify_password;
use crate::auth::routes::dto::{normalize_email, random_token};
use crate::auth::service::{
  verification_token_hash, verification_token_hash_prefix,
};
use crate::auth::totp::verify_code;
use crate::auth::user::current_user;

const CHANGE_EMAIL_TOKEN_TTL_SECS: i64 = 30 * 60;
const CHANGE_EMAIL_IDENTIFIER_PREFIX: &str = "change-email:";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEmailBody {
  /// New email address (`newEmail` or `new_email`).
  #[serde(alias = "new_email")]
  pub new_email: String,
  pub callback_url: Option<String>,
  /// Password re-check (required on the request path when the account
  /// has a credential password).
  pub password: Option<String>,
  /// TOTP code alternative (required when 2FA is enabled and no password is
  /// given, or when the account has no credential password).
  pub code: Option<String>,
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

  // The request path requires a fresh password or TOTP check on top of
  // the session. The confirm path below stays token-only (mail flow).
  let has_password = sqlx::query!(
    r#"SELECT password FROM accounts WHERE user_id = $1 AND provider_id = $2"#,
    su.user.id,
    CREDENTIAL_PROVIDER_ID,
  )
  .fetch_optional(pool)
  .await?
  .and_then(|r| r.password)
  .is_some_and(|p| !p.is_empty());
  let totp_verified = sqlx::query!(
    r#"SELECT secret, verified FROM two_factor WHERE user_id = $1"#,
    su.user.id,
  )
  .fetch_optional(pool)
  .await?
  .is_some_and(|r| r.verified);

  let password_ok = match body.password.as_deref().filter(|p| !p.is_empty()) {
    Some(pw) if has_password => {
      let row = sqlx::query!(
        r#"SELECT password FROM accounts WHERE user_id = $1 AND provider_id = $2"#,
        su.user.id,
        CREDENTIAL_PROVIDER_ID,
      )
      .fetch_optional(pool)
      .await?;
      match row.and_then(|r| r.password) {
        Some(stored) => verify_password(pw, &stored).map_err(|e| {
          topcoat::Error::from(std::io::Error::other(e.to_string()))
        })?,
        None => false,
      }
    }
    _ => false,
  };
  let totp_ok = if password_ok {
    false
  } else if let Some(code) = body
    .code
    .as_deref()
    .map(str::trim)
    .filter(|c| !c.is_empty())
  {
    if !totp_verified {
      false
    } else {
      let tf = sqlx::query!(
        r#"SELECT secret FROM two_factor WHERE user_id = $1"#,
        su.user.id,
      )
      .fetch_optional(pool)
      .await?;
      match tf {
        Some(tf) => verify_code(&tf.secret, code).map_err(|e| {
          topcoat::Error::from(std::io::Error::other(e.to_string()))
        })?,
        None => false,
      }
    }
  } else {
    false
  };

  // OAuth-only accounts without 2FA have nothing to re-check beyond the
  // session; everyone else must present password or TOTP.
  if (has_password || totp_verified) && !password_ok && !totp_ok {
    return Err(unauthorized().into());
  }

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
  let token_hash = verification_token_hash(&token);
  let token_hash_prefix =
    verification_token_hash_prefix(&token_hash).to_owned();
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
      user_id = %su.user.id,
      current_email = %su.user.email,
      new_email = %new_email,
      callback_url = ?body.callback_url,
      token_hash_prefix = %token_hash_prefix,
      confirm_path = "/api/auth/change-email/confirm",
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

  // Confirm stays token-only (no session) so mail links work logged-out.
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
