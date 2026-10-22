use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Json,
    error::{bad_request, unauthorized},
    route,
  },
};

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::{
  hash_password, validate_password, verify_password,
};
use crate::auth::session_store;
use crate::auth::user::current_user;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordBody {
  pub current_password: String,
  pub new_password: String,
  /// When false, keep other sessions for this user. Defaults to true:
  /// omitting the flag revokes every other session.
  #[serde(default)]
  pub revoke_other_sessions: Option<bool>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    tag = "auth",
    request_body = ChangePasswordBody,
    responses(
        (status = 200, description = "Password updated", body = OkStatus),
        (status = 401, description = "Missing session or wrong current password")
    )
)]
#[route(POST "/api/auth/change-password")]
pub async fn change_password(
  cx: &Cx,
  Json(body): Json<ChangePasswordBody>,
) -> Result<Json<OkStatus>> {
  let session = current_user(cx)
    .await
    .map_err(|e| crate::internal_error::internal_error_cx(cx, &e))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };
  crate::auth::step_up::forbid_impersonated(su)?;
  crate::auth::step_up::check_step_up_rate(cx)?;

  if body.current_password.is_empty() {
    return Err(bad_request("currentPassword is required").into());
  }
  // Defense-in-depth: the service validates authoritatively, but reject
  // weak input here too before any DB work.
  if let Err(err) = validate_password(
    &body.new_password,
    &[su.user.email.as_str(), su.user.name.as_str(), "lindaflor"],
  ) {
    return Err(bad_request(err.to_string()).into());
  }

  let pool = app_context::<PgPool>(cx);

  let account = sqlx::query!(
    r#"
        SELECT password
        FROM accounts
        WHERE user_id = $1 AND provider_id = $2
        "#,
    su.user.id,
    CREDENTIAL_PROVIDER_ID,
  )
  .fetch_optional(pool)
  .await?;

  let Some(account) = account else {
    return Err(unauthorized().into());
  };
  let Some(stored_hash) = account.password.as_deref() else {
    return Err(unauthorized().into());
  };

  let ok = verify_password(&body.current_password, stored_hash)
    .map_err(|e| crate::internal_error::internal_error_cx(cx, &e))?;
  if !ok {
    return Err(unauthorized().into());
  }

  let password_hash = hash_password(&body.new_password)?;

  sqlx::query!(
    r#"
        UPDATE accounts
        SET password = $1, updated_at = now()
        WHERE user_id = $2 AND provider_id = $3
        "#,
    password_hash,
    su.user.id,
    CREDENTIAL_PROVIDER_ID,
  )
  .execute(pool)
  .await?;

  if body.revoke_other_sessions.unwrap_or(true) {
    session_store::delete_all_for_user_except(pool, su.user.id, su.session_id)
      .await?;
  }

  Ok(Json(OkStatus { status: true }))
}
