use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::PrimitiveDateTime;
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
use crate::auth::routes::dto::{SessionPayload, client_meta, random_token};
use crate::auth::session_store;
use crate::auth::totp::{
  consume_backup_code, generate_backup_codes, generate_secret,
  hash_backup_codes, verify_code,
};
use crate::auth::user::{SessionUser, User, current_user};

pub const PENDING_2FA_PREFIX: &str = "2fa-pending:";
pub const PENDING_2FA_TTL_SECS: i64 = 10 * 60;

fn system_time_to_primitive(st: SystemTime) -> PrimitiveDateTime {
  let duration = st.duration_since(UNIX_EPOCH).unwrap_or_default();
  let odt =
    time::OffsetDateTime::from_unix_timestamp(duration.as_secs() as i64)
      .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

fn now_plus(secs: i64) -> PrimitiveDateTime {
  let odt = time::OffsetDateTime::now_utc() + time::Duration::seconds(secs);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

async fn require_session(cx: &Cx) -> Result<SessionUser> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  match session.as_ref() {
    Some(su) => Ok(su.clone()),
    None => Err(unauthorized().into()),
  }
}

async fn load_user(pool: &PgPool, user_id: Uuid) -> Result<User> {
  let row = sqlx::query!(
    r#"
        SELECT id, name, email, email_verified, image, two_factor_enabled, role,
               banned, ban_reason, ban_expires
        FROM users WHERE id = $1
        "#,
    user_id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| bad_request("user not found"))?;

  Ok(User {
    id: row.id,
    name: row.name,
    email: row.email,
    email_verified: row.email_verified,
    image: row.image,
    two_factor_enabled: row.two_factor_enabled,
    role: row.role,
    banned: row.banned,
    ban_reason: row.ban_reason,
    ban_expires: row.ban_expires,
  })
}

/// Create a short-lived pending-2FA verification; returns the opaque token.
pub async fn create_pending_2fa(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<String> {
  let token = random_token();
  let id = Uuid::now_v7();
  let identifier = format!("{PENDING_2FA_PREFIX}{user_id}");
  let expires_at = now_plus(PENDING_2FA_TTL_SECS);

  // Replace any prior pending token for this user.
  sqlx::query!(
    r#"DELETE FROM verifications WHERE identifier = $1"#,
    identifier,
  )
  .execute(pool)
  .await?;

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

  Ok(token)
}

async fn resolve_pending_user_id(
  pool: &PgPool,
  token: &str,
) -> Result<(Uuid, Uuid)> {
  let verification = sqlx::query!(
    r#"
        SELECT id, identifier, value, expires_at
        FROM verifications
        WHERE value = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    token,
    format!("{PENDING_2FA_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(unauthorized)?;

  let user_id_str = verification
    .identifier
    .strip_prefix(PENDING_2FA_PREFIX)
    .ok_or_else(unauthorized)?;
  let user_id = Uuid::parse_str(user_id_str).map_err(|_| unauthorized())?;
  Ok((verification.id, user_id))
}

async fn start_session_for_user(
  cx: &Cx,
  pool: &PgPool,
  user: User,
) -> Result<SessionPayload> {
  let session = session::start(cx).await?;
  let (ip, ua) = client_meta(cx);
  let session_id = session_store::insert_session(
    pool,
    &session,
    user.id,
    ip.as_deref(),
    ua.as_deref(),
    None,
  )
  .await?;

  Ok(SessionPayload::from(&SessionUser {
    user,
    session_id,
    expires_at: system_time_to_primitive(session.expires_at),
    impersonated_by: None,
  }))
}

async fn verify_account_password(
  pool: &PgPool,
  user_id: Uuid,
  password: &str,
) -> Result<bool> {
  let row = sqlx::query!(
    r#"SELECT password FROM accounts WHERE user_id = $1 AND provider_id = $2"#,
    user_id,
    CREDENTIAL_PROVIDER_ID,
  )
  .fetch_optional(pool)
  .await?;

  let Some(stored) = row.and_then(|r| r.password) else {
    return Ok(false);
  };
  Ok(verify_password(password, &stored)?)
}

// --- DTOs ---

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnableTwoFactorResponse {
  pub totp_uri: String,
  pub secret: String,
  pub backup_codes: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyTotpBody {
  pub code: String,
  /// Pending 2FA token from sign-in (`TWO_FACTOR_REQUIRED`).
  pub token: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisableTwoFactorBody {
  pub password: Option<String>,
  /// TOTP code (alternative to password).
  pub code: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyBackupCodeBody {
  pub code: String,
  /// Pending 2FA token from sign-in.
  pub token: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackupCodesResponse {
  pub backup_codes: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GenerateBackupCodesBody {
  /// Optional password re-check.
  pub password: Option<String>,
}

// --- Routes ---

#[utoipa::path(
    post,
    path = "/api/auth/two-factor/enable",
    tag = "auth",
    responses(
        (status = 200, description = "TOTP enrollment started (unverified)", body = EnableTwoFactorResponse)
    )
)]
#[route(POST "/api/auth/two-factor/enable")]
pub async fn enable(cx: &Cx) -> Result<Json<EnableTwoFactorResponse>> {
  let su = require_session(cx).await?;
  if su.user.two_factor_enabled {
    return Err(
      bad_request("two-factor authentication is already enabled").into(),
    );
  }

  let pool = app_context::<PgPool>(cx);
  let (secret, totp_uri) =
    generate_secret(&su.user.email).map_err(bad_request)?;
  let backup_codes = generate_backup_codes();
  let backup_stored = hash_backup_codes(&backup_codes);
  let id = Uuid::now_v7();

  sqlx::query!(r#"DELETE FROM two_factor WHERE user_id = $1"#, su.user.id)
    .execute(pool)
    .await?;

  sqlx::query!(
    r#"
        INSERT INTO two_factor (id, secret, backup_codes, user_id, verified)
        VALUES ($1, $2, $3, $4, false)
        "#,
    id,
    secret,
    backup_stored,
    su.user.id,
  )
  .execute(pool)
  .await?;

  Ok(Json(EnableTwoFactorResponse {
    totp_uri,
    secret,
    backup_codes,
  }))
}

#[utoipa::path(
    post,
    path = "/api/auth/two-factor/verify-totp",
    tag = "auth",
    request_body = VerifyTotpBody,
    responses(
        (status = 200, description = "TOTP verified (enable finished or session started)", body = SessionPayload),
        (status = 200, description = "Enable confirmed without new session payload shape", body = OkStatus)
    )
)]
#[route(POST "/api/auth/two-factor/verify-totp")]
pub async fn verify_totp(
  cx: &Cx,
  Json(body): Json<VerifyTotpBody>,
) -> Result<Json<serde_json::Value>> {
  let pool = app_context::<PgPool>(cx);
  let code = body.code.trim().to_string();
  if code.is_empty() {
    return Err(bad_request("code is required").into());
  }

  // Path A: complete sign-in with pending token.
  if let Some(token) = body.token.as_deref().filter(|t| !t.trim().is_empty()) {
    let (verification_id, user_id) =
      resolve_pending_user_id(pool, token.trim()).await?;
    let tf = sqlx::query!(
      r#"SELECT secret, verified FROM two_factor WHERE user_id = $1"#,
      user_id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(unauthorized)?;

    if !tf.verified {
      return Err(unauthorized().into());
    }
    if !verify_code(&tf.secret, &code).map_err(bad_request)? {
      return Err(unauthorized().into());
    }

    sqlx::query!("DELETE FROM verifications WHERE id = $1", verification_id)
      .execute(pool)
      .await?;

    let user = load_user(pool, user_id).await?;
    if user.banned {
      return Err(unauthorized().into());
    }
    let payload = start_session_for_user(cx, pool, user).await?;
    return Ok(Json(serde_json::to_value(payload)?));
  }

  // Path B: finish enable while already signed in.
  let su = require_session(cx).await?;
  if su.user.two_factor_enabled {
    return Err(
      bad_request("two-factor authentication is already enabled").into(),
    );
  }

  let tf = sqlx::query!(
    r#"SELECT id, secret, verified FROM two_factor WHERE user_id = $1"#,
    su.user.id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| bad_request("call /two-factor/enable first"))?;

  if tf.verified {
    return Err(
      bad_request("two-factor authentication is already verified").into(),
    );
  }
  if !verify_code(&tf.secret, &code).map_err(bad_request)? {
    return Err(unauthorized().into());
  }

  sqlx::query!(
    r#"UPDATE two_factor SET verified = true WHERE id = $1"#,
    tf.id,
  )
  .execute(pool)
  .await?;

  sqlx::query!(
        r#"UPDATE users SET two_factor_enabled = true, updated_at = now() WHERE id = $1"#,
        su.user.id,
    )
    .execute(pool)
    .await?;

  let mut user = su.user.clone();
  user.two_factor_enabled = true;
  let payload = SessionPayload::from(&SessionUser {
    user,
    session_id: su.session_id,
    expires_at: su.expires_at,
    impersonated_by: su.impersonated_by,
  });
  Ok(Json(serde_json::to_value(payload)?))
}

#[utoipa::path(
    post,
    path = "/api/auth/two-factor/disable",
    tag = "auth",
    request_body = DisableTwoFactorBody,
    responses(
        (status = 200, description = "Two-factor disabled", body = OkStatus)
    )
)]
#[route(POST "/api/auth/two-factor/disable")]
pub async fn disable(
  cx: &Cx,
  Json(body): Json<DisableTwoFactorBody>,
) -> Result<Json<OkStatus>> {
  let su = require_session(cx).await?;
  let pool = app_context::<PgPool>(cx);

  let password_ok = match body.password.as_deref().filter(|p| !p.is_empty()) {
    Some(pw) => verify_account_password(pool, su.user.id, pw).await?,
    None => false,
  };

  let totp_ok = if password_ok {
    false
  } else if let Some(code) =
    body.code.as_deref().filter(|c| !c.trim().is_empty())
  {
    let tf = sqlx::query!(
      r#"SELECT secret, verified FROM two_factor WHERE user_id = $1"#,
      su.user.id,
    )
    .fetch_optional(pool)
    .await?;
    match tf {
      Some(tf) if tf.verified => {
        verify_code(&tf.secret, code).map_err(bad_request)?
      }
      _ => false,
    }
  } else {
    false
  };

  if !password_ok && !totp_ok {
    return Err(unauthorized().into());
  }

  sqlx::query!(r#"DELETE FROM two_factor WHERE user_id = $1"#, su.user.id)
    .execute(pool)
    .await?;

  sqlx::query!(
        r#"UPDATE users SET two_factor_enabled = false, updated_at = now() WHERE id = $1"#,
        su.user.id,
    )
    .execute(pool)
    .await?;

  Ok(Json(OkStatus { status: true }))
}

#[utoipa::path(
    post,
    path = "/api/auth/two-factor/verify-backup-code",
    tag = "auth",
    request_body = VerifyBackupCodeBody,
    responses(
        (status = 200, description = "Backup code accepted", body = SessionPayload)
    )
)]
#[route(POST "/api/auth/two-factor/verify-backup-code")]
pub async fn verify_backup_code(
  cx: &Cx,
  Json(body): Json<VerifyBackupCodeBody>,
) -> Result<Json<serde_json::Value>> {
  let pool = app_context::<PgPool>(cx);
  let code = body.code.trim().to_string();
  if code.is_empty() {
    return Err(bad_request("code is required").into());
  }

  // Pending sign-in path.
  if let Some(token) = body.token.as_deref().filter(|t| !t.trim().is_empty()) {
    let (verification_id, user_id) =
      resolve_pending_user_id(pool, token.trim()).await?;
    let tf = sqlx::query!(
      r#"SELECT id, backup_codes, verified FROM two_factor WHERE user_id = $1"#,
      user_id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(unauthorized)?;

    if !tf.verified {
      return Err(unauthorized().into());
    }

    let stored =
      consume_backup_code(&tf.backup_codes, &code).ok_or_else(unauthorized)?;
    sqlx::query!(
      r#"UPDATE two_factor SET backup_codes = $2 WHERE id = $1"#,
      tf.id,
      stored,
    )
    .execute(pool)
    .await?;

    sqlx::query!("DELETE FROM verifications WHERE id = $1", verification_id)
      .execute(pool)
      .await?;

    let user = load_user(pool, user_id).await?;
    if user.banned {
      return Err(unauthorized().into());
    }
    let payload = start_session_for_user(cx, pool, user).await?;
    return Ok(Json(serde_json::to_value(payload)?));
  }

  // Logged-in confirmation: consume a backup code.
  let su = require_session(cx).await?;
  let tf = sqlx::query!(
    r#"SELECT id, backup_codes, verified FROM two_factor WHERE user_id = $1"#,
    su.user.id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| bad_request("two-factor is not configured"))?;

  if !tf.verified {
    return Err(bad_request("two-factor is not verified").into());
  }

  let stored =
    consume_backup_code(&tf.backup_codes, &code).ok_or_else(unauthorized)?;
  sqlx::query!(
    r#"UPDATE two_factor SET backup_codes = $2 WHERE id = $1"#,
    tf.id,
    stored,
  )
  .execute(pool)
  .await?;

  Ok(Json(serde_json::to_value(OkStatus { status: true })?))
}

#[utoipa::path(
    post,
    path = "/api/auth/two-factor/generate-backup-codes",
    tag = "auth",
    request_body = GenerateBackupCodesBody,
    responses(
        (status = 200, description = "New backup codes", body = BackupCodesResponse)
    )
)]
#[route(POST "/api/auth/two-factor/generate-backup-codes")]
pub async fn generate_backup_codes_route(
  cx: &Cx,
  Json(body): Json<GenerateBackupCodesBody>,
) -> Result<Json<BackupCodesResponse>> {
  let su = require_session(cx).await?;
  if !su.user.two_factor_enabled {
    return Err(bad_request("two-factor authentication is not enabled").into());
  }

  let pool = app_context::<PgPool>(cx);

  if let Some(pw) = body.password.as_deref()
    && (pw.is_empty() || !verify_account_password(pool, su.user.id, pw).await?)
  {
    return Err(unauthorized().into());
  }

  let tf = sqlx::query!(
    r#"SELECT id FROM two_factor WHERE user_id = $1 AND verified = true"#,
    su.user.id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| bad_request("two-factor is not configured"))?;

  let backup_codes = generate_backup_codes();
  let stored = hash_backup_codes(&backup_codes);
  sqlx::query!(
    r#"UPDATE two_factor SET backup_codes = $2 WHERE id = $1"#,
    tf.id,
    stored,
  )
  .execute(pool)
  .await?;

  Ok(Json(BackupCodesResponse { backup_codes }))
}
