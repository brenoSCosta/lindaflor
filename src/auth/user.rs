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

/// Whether `(banned, ban_expires)` counts as banned right now.
///
/// Single source of truth for ban enforcement:
/// permanent bans (`ban_expires = NULL`) stay banned, temporary bans lift as
/// soon as `ban_expires` passes. Use this everywhere a ban is checked
/// (`current_user`, sign-in, impersonation, 2FA completion) so expired temp
/// bans lift on every path instead of drifting apart.
pub fn is_currently_banned(
  banned: bool,
  ban_expires: Option<PrimitiveDateTime>,
) -> bool {
  if !banned {
    return false;
  }
  match ban_expires {
    None => true,
    Some(expires) => expires.assume_utc() > time::OffsetDateTime::now_utc(),
  }
}

/// Resolve the signed-in user for this request, or `None` if unauthenticated.
///
/// - Looks up the cookie token via [`session::token_hash`]
/// - Loads the session + user from Postgres (hex TokenHash)
/// - Rejects currently-banned users (returns `None`; expired temp bans lift
///   via [`is_currently_banned`])
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

  if is_currently_banned(row.banned, row.ban_expires) {
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

  let su = SessionUser::from(row);
  crate::logging::request_store::set_request_user(
    cx,
    crate::logging::request_store::RequestUser {
      user_id: su.user.id.to_string(),
      session_id: su.session_id.to_string(),
      role: su.user.role.clone(),
    },
  );
  Ok(Some(su))
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

#[cfg(test)]
mod tests {
  use super::*;

  fn primitive_hours_from_now(hours: i64) -> PrimitiveDateTime {
    let odt = time::OffsetDateTime::now_utc() + time::Duration::hours(hours);
    PrimitiveDateTime::new(odt.date(), odt.time())
  }

  #[test]
  fn ban_expiry_boundary() {
    // Never banned: no ban regardless of expiry value.
    assert!(!is_currently_banned(false, None));
    assert!(!is_currently_banned(
      false,
      Some(primitive_hours_from_now(1))
    ));
    // Permanent ban (no expiry) stays banned.
    assert!(is_currently_banned(true, None));
    // Future expiry is still banned; past expiry lifts the ban.
    assert!(is_currently_banned(true, Some(primitive_hours_from_now(1))));
    assert!(!is_currently_banned(
      true,
      Some(primitive_hours_from_now(-1))
    ));
  }
}
