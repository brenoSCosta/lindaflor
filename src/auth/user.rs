use std::time::{Duration, SystemTime};

use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context, memoize},
  session,
};
use uuid::Uuid;

use super::session_config::SESSION_UPDATE_AGE;
use super::session_store::{self, SessionWithUser};

/// Application user as stored in `users`.
#[derive(Debug, Clone)]
pub struct User {
  pub id: Uuid,
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

/// Authenticated principal for the current request (user + session metadata).
#[derive(Debug, Clone)]
pub struct SessionUser {
  pub user: User,
  pub session_id: Uuid,
  pub expires_at: PrimitiveDateTime,
  pub impersonated_by: Option<Uuid>,
}

impl From<SessionWithUser> for SessionUser {
  fn from(row: SessionWithUser) -> Self {
    Self {
      user: User {
        id: row.user_id,
        name: row.name,
        email: row.email,
        email_verified: row.email_verified,
        image: row.image,
        two_factor_enabled: row.two_factor_enabled,
        role: row.role,
        banned: row.banned,
        ban_reason: row.ban_reason,
        ban_expires: row.ban_expires,
      },
      session_id: row.session_id,
      expires_at: row.expires_at,
      impersonated_by: row.impersonated_by,
    }
  }
}

fn primitive_to_system_time(dt: PrimitiveDateTime) -> SystemTime {
  let odt = dt.assume_utc();
  let secs = odt.unix_timestamp();
  if secs >= 0 {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
  } else {
    SystemTime::UNIX_EPOCH
  }
}

/// Resolve the signed-in user for this request, or `None` if unauthenticated.
///
/// - Looks up the cookie token via [`session::token_hash`]
/// - Loads the session + user from Postgres (hex TokenHash)
/// - Rejects banned users (returns `None`)
/// - Slides expiry about once per day ([`SESSION_UPDATE_AGE`])
#[memoize(as_ref)]
pub async fn current_user(cx: &Cx) -> Result<Option<SessionUser>> {
  let Some(hash) = session::token_hash(cx).await? else {
    return Ok(None);
  };

  let pool = app_context::<PgPool>(cx);
  let Some(row) = session_store::find_by_token_hash(pool, &hash).await? else {
    return Ok(None);
  };

  if row.banned {
    return Ok(None);
  }

  let updated_at = primitive_to_system_time(row.updated_at);
  let age = SystemTime::now()
    .duration_since(updated_at)
    .unwrap_or(Duration::ZERO);

  if age >= SESSION_UPDATE_AGE
    && let Some(refreshed) = session::refresh(cx).await?
  {
    session_store::update_expiry(
      pool,
      &refreshed.token_hash,
      refreshed.expires_at,
    )
    .await?;
  }

  Ok(Some(SessionUser::from(row)))
}

/// Owned session lookup for callers that cannot use the memoized `&Error` / `&Option`.
pub async fn current_user_owned(cx: &Cx) -> Result<Option<SessionUser>> {
  Ok(
    current_user(cx)
      .await
      .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?
      .clone(),
  )
}
