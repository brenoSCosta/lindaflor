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

use crate::auth::routes::dto::AuthUserJson;
use crate::auth::user::{User, current_user};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserBody {
  pub name: Option<String>,
  /// Absent = leave unchanged; `null` (or `""`) clears. Any other value is
  /// rejected: avatars are uploaded through `/settings/avatar`.
  #[serde(default, deserialize_with = "present_or_null")]
  pub image: Option<Option<String>>,
}

/// Keep `"image": null` distinct from an absent field (plain `Option`
/// would collapse both to `None`).
fn present_or_null<'de, D>(de: D) -> Result<Option<Option<String>>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  Option::<String>::deserialize(de).map(Some)
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserResponse {
  pub user: AuthUserJson,
}

#[utoipa::path(
    post,
    path = "/api/auth/update-user",
    tag = "auth",
    request_body = UpdateUserBody,
    responses(
        (status = 200, description = "User updated", body = UpdateUserResponse),
        (status = 401, description = "Missing session")
    )
)]
#[route(POST "/api/auth/update-user")]
pub async fn update_user(
  cx: &Cx,
  Json(body): Json<UpdateUserBody>,
) -> Result<Json<UpdateUserResponse>> {
  let session = current_user(cx)
    .await
    .map_err(|e| crate::internal_error::internal_error_cx(cx, &e))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };
  crate::auth::step_up::forbid_impersonated(su)?;

  if body.name.is_none() && body.image.is_none() {
    return Err(bad_request("name or image is required").into());
  }

  let name = match body.name.as_deref() {
    None => None,
    Some(raw) => Some(
      crate::auth::validation::validate_name(raw)
        .map_err(|err| bad_request(err.to_string()))?,
    ),
  };

  // `users.image` only ever holds a server-issued avatar key (written by
  // `/settings/avatar`) or a Google profile picture. A client-supplied value
  // would let anyone point it at another bucket object and then have the
  // avatar replace/remove flow delete it, so the API may only clear it.
  if let Some(Some(image)) = body.image.as_ref()
    && !image.trim().is_empty()
  {
    return Err(
      bad_request("image can only be cleared; upload avatars in settings")
        .into(),
    );
  }

  let pool = app_context::<PgPool>(cx);

  if let Some(name) = name {
    sqlx::query!(
      r#"
            UPDATE users
            SET name = $2, updated_at = now()
            WHERE id = $1
            "#,
      su.user.id,
      name,
    )
    .execute(pool)
    .await?;
  }
  if body.image.is_some() {
    // Deletes the object only when it is this user's own avatar key.
    crate::auth::avatar::remove_avatar(
      pool,
      &crate::app::utils::object_store(cx),
      su.user.id,
    )
    .await?;
  }

  let row = sqlx::query!(
    r#"
        SELECT
            id, name, email, email_verified, image, two_factor_enabled,
            role, banned, ban_reason, ban_expires
        FROM users
        WHERE id = $1
        "#,
    su.user.id,
  )
  .fetch_one(pool)
  .await?;

  let user = User {
    id: row.id,
    name: row.name,
    email: row.email,
    email_verified: row.email_verified,
    image: row.image,
    two_factor_enabled: row.two_factor_enabled,
    role: row.role,
    banned: row.banned,
    ban_reason: row.ban_reason,
    ban_expires: row.ban_expires,
  };

  Ok(Json(UpdateUserResponse {
    user: AuthUserJson::from(&user),
  }))
}
