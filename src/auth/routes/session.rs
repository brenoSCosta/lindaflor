//! GET `/api/auth/get-session`

use topcoat::{
  Result,
  context::Cx,
  router::{content::Json, route},
};

use crate::auth::routes::dto::SessionPayload;
use crate::auth::user::current_user;

#[utoipa::path(
    get,
    path = "/api/auth/get-session",
    tag = "auth",
    responses(
        (status = 200, description = "Current session, or null when unauthenticated", body = Option<SessionPayload>)
    )
)]
#[route(GET "/api/auth/get-session")]
pub async fn get_session(cx: &Cx) -> Result<Json<Option<SessionPayload>>> {
  // `#[memoize(as_ref)]` → `Result<&Option<SessionUser>, &Error>`
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  Ok(Json(session.as_ref().map(SessionPayload::from)))
}
