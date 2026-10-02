//! POST `/api/auth/sign-out`

use serde::Serialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{content::Json, route},
  session,
};

use crate::auth::session_store;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignOutResponse {
  pub success: bool,
}

#[utoipa::path(
    post,
    path = "/api/auth/sign-out",
    tag = "auth",
    responses(
        (status = 200, description = "Signed out", body = SignOutResponse)
    )
)]
#[route(POST "/api/auth/sign-out")]
pub async fn sign_out(cx: &Cx) -> Result<Json<SignOutResponse>> {
  let pool = app_context::<PgPool>(cx);
  if let Some(hash) = session::stop(cx).await? {
    session_store::delete_by_token_hash(pool, &hash).await?;
  }
  Ok(Json(SignOutResponse { success: true }))
}
