use topcoat::{
  Result,
  context::Cx,
  router::{error::RouterErrorExt, href},
};

use crate::auth::service;
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

/// Resolve the signed-in admin or redirect to `/dashboard` (anonymous users
/// go to `/login` via [`require_user`]).
///
/// Keep calling this inside every `shard`/`procedure`/`route` handler: those
/// endpoints bypass page/layout guards, so the layer alone is not enough.
pub async fn require_admin(cx: &Cx) -> Result<SessionUser> {
  let su = require_user(cx).await?;
  if !service::is_admin(su.user.role.as_deref()) {
    return Err(
      topcoat::router::error::see_other(
        href!(crate::app::dashboard::page).resolve(cx),
      )
      .into(),
    );
  }
  Ok(su)
}
