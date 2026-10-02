//! Shared helpers for HTML auth pages.

use topcoat::{Result, context::Cx, router::error::RouterErrorExt};

use lindaflor::auth::user::{SessionUser, current_user_owned};

/// Resolve the signed-in user or redirect to `/login`.
pub async fn require_user(cx: &Cx) -> Result<SessionUser> {
  Ok(current_user_owned(cx).await?.ok_or_redirect("/login")?)
}

/// Optional current user.
pub async fn optional_user(cx: &Cx) -> Result<Option<SessionUser>> {
  current_user_owned(cx).await
}

pub fn encode_query(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        out.push(b as char)
      }
      b' ' => out.push_str("%20"),
      _ => out.push_str(&format!("%{b:02X}")),
    }
  }
  out
}
