//! Persist / look up / delete sessions in Postgres.
//!
//! `sessions.token` stores the **hex-encoded** SHA-256 [`TokenHash`] bytes
//! (64 lowercase hex chars), never the raw client token.

use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::session::{Session, TokenHash};
use uuid::Uuid;

/// Hex-encode a [`TokenHash`] for the `sessions.token` column.
///
/// [`TokenHash`] has no `Display`; encode the 32 bytes yourself.
pub fn token_hash_hex(hash: &TokenHash) -> String {
  hex::encode(**hash)
}

fn system_time_to_primitive(st: SystemTime) -> PrimitiveDateTime {
  let duration = st.duration_since(UNIX_EPOCH).unwrap_or_default();
  let secs = duration.as_secs() as i64;
  let odt = time::OffsetDateTime::from_unix_timestamp(secs)
    .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

/// Insert a new session row after [`topcoat::session::start`].
///
/// Pass `impersonated_by` when an admin is acting as another user.
pub async fn insert_session(
  pool: &PgPool,
  session: &Session,
  user_id: Uuid,
  ip_address: Option<&str>,
  user_agent: Option<&str>,
  impersonated_by: Option<Uuid>,
) -> Result<Uuid, sqlx::Error> {
  let id = Uuid::now_v7();
  let token = token_hash_hex(&session.token_hash);
  let expires_at = system_time_to_primitive(session.expires_at);

  sqlx::query!(
    r#"
        INSERT INTO sessions (
            id, expires_at, token, created_at, updated_at,
            ip_address, user_agent, user_id, impersonated_by
        )
        VALUES ($1, $2, $3, now(), now(), $4, $5, $6, $7)
        "#,
    id,
    expires_at,
    token,
    ip_address,
    user_agent,
    user_id,
    impersonated_by,
  )
  .execute(pool)
  .await?;

  Ok(id)
}

/// Session row joined with its user (unexpired only).
#[derive(Debug, Clone)]
pub struct SessionWithUser {
  pub session_id: Uuid,
  pub expires_at: PrimitiveDateTime,
  pub updated_at: PrimitiveDateTime,
  pub impersonated_by: Option<Uuid>,
  pub user_id: Uuid,
  pub name: String,
  pub email: String,
  pub email_verified: bool,
  pub image: Option<String>,
  pub two_factor_enabled: bool,
  pub role: Option<String>,
  pub banned: bool,
  pub ban_reason: Option<String>,
  pub ban_expires: Option<PrimitiveDateTime>,
}

/// Look up a non-expired session by token hash hex.
pub async fn find_by_token_hash(
  pool: &PgPool,
  hash: &TokenHash,
) -> Result<Option<SessionWithUser>, sqlx::Error> {
  let token = token_hash_hex(hash);

  let row = sqlx::query!(
    r#"
        SELECT
            s.id AS session_id,
            s.expires_at,
            s.updated_at,
            s.impersonated_by,
            u.id AS user_id,
            u.name,
            u.email,
            u.email_verified,
            u.image,
            u.two_factor_enabled,
            u.role,
            u.banned,
            u.ban_reason,
            u.ban_expires
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.token = $1
          AND s.expires_at > now()
        "#,
    token,
  )
  .fetch_optional(pool)
  .await?;

  Ok(row.map(|r| SessionWithUser {
    session_id: r.session_id,
    expires_at: r.expires_at,
    updated_at: r.updated_at,
    impersonated_by: r.impersonated_by,
    user_id: r.user_id,
    name: r.name,
    email: r.email,
    email_verified: r.email_verified,
    image: r.image,
    two_factor_enabled: r.two_factor_enabled,
    role: r.role,
    banned: r.banned,
    ban_reason: r.ban_reason,
    ban_expires: r.ban_expires,
  }))
}

/// Delete a session by its token hash (logout).
pub async fn delete_by_token_hash(
  pool: &PgPool,
  hash: &TokenHash,
) -> Result<(), sqlx::Error> {
  let token = token_hash_hex(hash);
  sqlx::query!("DELETE FROM sessions WHERE token = $1", token)
    .execute(pool)
    .await?;
  Ok(())
}

/// Delete a session by id when it belongs to `user_id`.
///
/// Returns `true` when a row was deleted.
pub async fn delete_owned_session(
  pool: &PgPool,
  user_id: Uuid,
  session_id: Uuid,
) -> Result<bool, sqlx::Error> {
  let result = sqlx::query!(
    "DELETE FROM sessions WHERE id = $1 AND user_id = $2",
    session_id,
    user_id,
  )
  .execute(pool)
  .await?;
  Ok(result.rows_affected() > 0)
}

/// Delete every session for a user except the one with this token hash hex.
pub async fn delete_all_for_user_except_token(
  pool: &PgPool,
  user_id: Uuid,
  keep_token_hex: &str,
) -> Result<(), sqlx::Error> {
  sqlx::query!(
    "DELETE FROM sessions WHERE user_id = $1 AND token <> $2",
    user_id,
    keep_token_hex,
  )
  .execute(pool)
  .await?;
  Ok(())
}

/// Delete every session for a user (e.g. after password reset).
pub async fn delete_all_for_user(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query!("DELETE FROM sessions WHERE user_id = $1", user_id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Delete every session for a user except the current one (`change-password` revokeOtherSessions).
pub async fn delete_all_for_user_except(
  pool: &PgPool,
  user_id: Uuid,
  keep_session_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query!(
    "DELETE FROM sessions WHERE user_id = $1 AND id <> $2",
    user_id,
    keep_session_id,
  )
  .execute(pool)
  .await?;
  Ok(())
}

/// Push `expires_at` forward after [`topcoat::session::refresh`].
pub async fn update_expiry(
  pool: &PgPool,
  hash: &TokenHash,
  expires_at: SystemTime,
) -> Result<(), sqlx::Error> {
  let token = token_hash_hex(hash);
  let expires_at = system_time_to_primitive(expires_at);
  sqlx::query!(
    r#"
        UPDATE sessions
        SET expires_at = $2, updated_at = now()
        WHERE token = $1
        "#,
    token,
    expires_at,
  )
  .execute(pool)
  .await?;
  Ok(())
}
