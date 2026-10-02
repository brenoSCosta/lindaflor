//! POST `/api/auth/update-user` (authenticated).

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
  /// Absent = leave unchanged; `null` clears; string sets the image URL.
  #[serde(default)]
  pub image: Option<Option<String>>,
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
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  if body.name.is_none() && body.image.is_none() {
    return Err(bad_request("name or image is required").into());
  }

  let name = body
    .name
    .as_deref()
    .map(str::trim)
    .filter(|n| !n.is_empty());
  if body.name.is_some() && name.is_none() {
    return Err(bad_request("name must not be empty").into());
  }

  let pool = app_context::<PgPool>(cx);

  match (name, body.image.as_ref()) {
    (Some(name), Some(image)) => {
      let image = image.as_deref().map(str::trim).filter(|s| !s.is_empty());
      sqlx::query!(
        r#"
                UPDATE users
                SET name = $2, image = $3, updated_at = now()
                WHERE id = $1
                "#,
        su.user.id,
        name,
        image,
      )
      .execute(pool)
      .await?;
    }
    (Some(name), None) => {
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
    (None, Some(image)) => {
      let image = image.as_deref().map(str::trim).filter(|s| !s.is_empty());
      sqlx::query!(
        r#"
                UPDATE users
                SET image = $2, updated_at = now()
                WHERE id = $1
                "#,
        su.user.id,
        image,
      )
      .execute(pool)
      .await?;
    }
    (None, None) => unreachable!(),
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
