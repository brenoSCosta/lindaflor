//! Shared camelCase JSON shapes matching Better Auth client expectations.

use serde::{Deserialize, Serialize};
use time::PrimitiveDateTime;
use uuid::Uuid;

use crate::auth::user::{SessionUser, User};

/// Better Auth-style user object.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserJson {
  pub id: String,
  pub name: String,
  pub email: String,
  pub email_verified: bool,
  pub image: Option<String>,
  pub role: Option<String>,
  pub two_factor_enabled: bool,
  pub banned: bool,
  pub ban_reason: Option<String>,
  pub ban_expires: Option<String>,
}

/// Better Auth-style session object (no raw token).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionJson {
  pub id: String,
  pub expires_at: String,
  pub user_id: String,
  pub impersonated_by: Option<String>,
}

/// `{ user, session }` payload returned by sign-up / sign-in / get-session.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionPayload {
  pub user: AuthUserJson,
  pub session: AuthSessionJson,
}

pub fn format_primitive(dt: PrimitiveDateTime) -> String {
  dt.assume_utc()
    .format(&time::format_description::well_known::Rfc3339)
    .unwrap_or_else(|_| dt.to_string())
}

impl From<&User> for AuthUserJson {
  fn from(user: &User) -> Self {
    Self {
      id: user.id.to_string(),
      name: user.name.clone(),
      email: user.email.clone(),
      email_verified: user.email_verified,
      image: user.image.clone(),
      role: user.role.clone(),
      two_factor_enabled: user.two_factor_enabled,
      banned: user.banned,
      ban_reason: user.ban_reason.clone(),
      ban_expires: user.ban_expires.map(format_primitive),
    }
  }
}

impl AuthSessionJson {
  pub fn new(
    session_id: Uuid,
    expires_at: PrimitiveDateTime,
    user_id: Uuid,
    impersonated_by: Option<Uuid>,
  ) -> Self {
    Self {
      id: session_id.to_string(),
      expires_at: format_primitive(expires_at),
      user_id: user_id.to_string(),
      impersonated_by: impersonated_by.map(|id| id.to_string()),
    }
  }
}

impl From<&SessionUser> for SessionPayload {
  fn from(su: &SessionUser) -> Self {
    Self {
      user: AuthUserJson::from(&su.user),
      session: AuthSessionJson::new(
        su.session_id,
        su.expires_at,
        su.user.id,
        su.impersonated_by,
      ),
    }
  }
}

pub fn client_meta(
  cx: &topcoat::context::Cx,
) -> (Option<String>, Option<String>) {
  use topcoat::router::request::headers;

  let headers = headers(cx);
  let user_agent = headers
    .get("user-agent")
    .and_then(|v| v.to_str().ok())
    .map(str::to_owned);
  let ip_address = headers
    .get("x-forwarded-for")
    .and_then(|v| v.to_str().ok())
    .and_then(|v| v.split(',').next())
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned)
    .or_else(|| {
      headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
    });
  (ip_address, user_agent)
}

pub fn normalize_email(email: &str) -> String {
  email.trim().to_ascii_lowercase()
}

pub fn random_token() -> String {
  use rand::RngCore;
  let mut bytes = [0u8; 32];
  rand::thread_rng().fill_bytes(&mut bytes);
  hex::encode(bytes)
}
