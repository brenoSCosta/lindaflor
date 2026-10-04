use topcoat::{
  Result,
  context::Cx,
  router::{error::RouterErrorExt, href},
};

use crate::auth::user::{SessionUser, current_user_owned};

/// Resolve the signed-in user or redirect to `/login`.
pub async fn require_user(cx: &Cx) -> Result<SessionUser> {
  Ok(
    current_user_owned(cx)
      .await?
      .ok_or_redirect(href!(crate::app::login::page).resolve(cx))?,
  )
}

/// Optional current user.
pub async fn optional_user(cx: &Cx) -> Result<Option<SessionUser>> {
  current_user_owned(cx).await
}
