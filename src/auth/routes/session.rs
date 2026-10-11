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
    .map_err(|e| crate::internal_error::internal_error_cx(cx, &e))?;
  Ok(Json(session.as_ref().map(SessionPayload::from)))
}
