//! Email verification: send + verify.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Json,
    error::{bad_request, unauthorized},
    query_params, route,
  },
};
use uuid::Uuid;

use crate::auth::routes::dto::{normalize_email, random_token};
use crate::auth::user::current_user;

const VERIFY_TOKEN_TTL_SECS: i64 = 60 * 60;
const VERIFY_IDENTIFIER_PREFIX: &str = "email-verification:";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendVerificationEmailBody {
  pub email: Option<String>,
  pub callback_url: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[derive(Debug)]
#[query_params(error = bad_request)]
struct VerifyEmailQuery {
  token: Option<String>,
  #[serde(rename = "callbackURL")]
  callback_url: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyEmailBody {
  pub token: String,
}

#[utoipa::path(
    post,
    path = "/api/auth/send-verification-email",
    tag = "auth",
    request_body = SendVerificationEmailBody,
    responses(
        (status = 200, description = "Verification email queued (logged in dev)", body = OkStatus)
    )
)]
#[route(POST "/api/auth/send-verification-email")]
pub async fn send_verification_email(
  cx: &Cx,
  Json(body): Json<SendVerificationEmailBody>,
) -> Result<Json<OkStatus>> {
  let pool = app_context::<PgPool>(cx);

  let email = if let Some(email) =
    body.email.as_deref().filter(|e| !e.trim().is_empty())
  {
    normalize_email(email)
  } else {
    let session = current_user(cx).await.map_err(|e| {
      topcoat::Error::from(std::io::Error::other(e.to_string()))
    })?;
    match session.as_ref() {
      Some(su) => su.user.email.clone(),
      None => return Err(bad_request("email is required").into()),
    }
  };

  let user = sqlx::query!(
    r#"SELECT id, name, email_verified FROM users WHERE email = $1"#,
    email
  )
  .fetch_optional(pool)
  .await?;

  // Anti-enumeration: succeed even when the user is missing or already verified.
  if let Some(user) = user
    && !user.email_verified
  {
    let token = random_token();
    let id = Uuid::now_v7();
    let identifier = format!("{VERIFY_IDENTIFIER_PREFIX}{email}");
    let expires_at = time::OffsetDateTime::now_utc()
      + time::Duration::seconds(VERIFY_TOKEN_TTL_SECS);
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

    tracing::info!(
        user_id = %user.id,
        email = %email,
        callback_url = ?body.callback_url,
        verification_token = %token,
        "email verification token created (mail not wired yet)"
    );
  }

  Ok(Json(OkStatus { status: true }))
}

#[utoipa::path(
    get,
    path = "/api/auth/verify-email",
    tag = "auth",
    params(
        ("token" = String, Query, description = "Verification token"),
        ("callbackURL" = Option<String>, Query, description = "Optional redirect hint")
    ),
    responses(
        (status = 200, description = "Email verified", body = OkStatus)
    )
)]
#[route(GET "/api/auth/verify-email")]
pub async fn verify_email(cx: &Cx) -> Result<Json<OkStatus>> {
  let query = query_params::<VerifyEmailQuery>(cx)?;
  let Some(token) = query.token.clone().filter(|t| !t.is_empty()) else {
    return Err(bad_request("token is required").into());
  };
  consume_verification_token(cx, &token).await?;
  let _ = query.callback_url;
  Ok(Json(OkStatus { status: true }))
}

#[utoipa::path(
    post,
    path = "/api/auth/verify-email",
    tag = "auth",
    request_body = VerifyEmailBody,
    responses(
        (status = 200, description = "Email verified", body = OkStatus)
    )
)]
#[route(POST "/api/auth/verify-email")]
pub async fn verify_email_post(
  cx: &Cx,
  Json(body): Json<VerifyEmailBody>,
) -> Result<Json<OkStatus>> {
  if body.token.trim().is_empty() {
    return Err(bad_request("token is required").into());
  }
  consume_verification_token(cx, &body.token).await?;
  Ok(Json(OkStatus { status: true }))
}

async fn consume_verification_token(cx: &Cx, token: &str) -> Result<()> {
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
    format!("{VERIFY_IDENTIFIER_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?;

  let Some(verification) = verification else {
    return Err(bad_request("invalid or expired token").into());
  };

  let email = verification
    .identifier
    .strip_prefix(VERIFY_IDENTIFIER_PREFIX)
    .unwrap_or(&verification.identifier);
  let email = normalize_email(email);

  let updated = sqlx::query!(
    r#"
        UPDATE users
        SET email_verified = true, updated_at = now()
        WHERE email = $1
        "#,
    email,
  )
  .execute(pool)
  .await?;

  if updated.rows_affected() == 0 {
    return Err(unauthorized().into());
  }

  sqlx::query!("DELETE FROM verifications WHERE id = $1", verification.id)
    .execute(pool)
    .await?;

  Ok(())
}
