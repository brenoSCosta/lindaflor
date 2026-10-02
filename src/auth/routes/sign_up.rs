//! POST `/api/auth/sign-up/email`

use std::time::{SystemTime, UNIX_EPOCH};

use http::StatusCode;
use serde::Deserialize;
use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{content::Json, error::bad_request, route},
  session,
};
use uuid::Uuid;

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::hash_password;
use crate::auth::routes::dto::{SessionPayload, client_meta, normalize_email};
use crate::auth::session_store;
use crate::auth::user::{SessionUser, User};

const MIN_PASSWORD_LEN: usize = 8;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignUpEmailBody {
  pub name: String,
  pub email: String,
  pub password: String,
}

fn system_time_to_primitive(st: SystemTime) -> PrimitiveDateTime {
  let duration = st.duration_since(UNIX_EPOCH).unwrap_or_default();
  let odt =
    time::OffsetDateTime::from_unix_timestamp(duration.as_secs() as i64)
      .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

#[utoipa::path(
    post,
    path = "/api/auth/sign-up/email",
    tag = "auth",
    request_body = SignUpEmailBody,
    responses(
        (status = 200, description = "User created and signed in", body = SessionPayload)
    )
)]
#[route(POST "/api/auth/sign-up/email")]
pub async fn sign_up_email(
  cx: &Cx,
  Json(body): Json<SignUpEmailBody>,
) -> Result<(StatusCode, Json<SessionPayload>)> {
  let name = body.name.trim().to_string();
  let email = normalize_email(&body.email);
  let password = body.password;

  if name.is_empty() {
    return Err(bad_request("name is required").into());
  }
  if email.is_empty() || !email.contains('@') {
    return Err(bad_request("valid email is required").into());
  }
  if password.len() < MIN_PASSWORD_LEN {
    return Err(
      bad_request(format!(
        "password must be at least {MIN_PASSWORD_LEN} characters"
      ))
      .into(),
    );
  }

  let pool = app_context::<PgPool>(cx);

  if sqlx::query_scalar!(
    r#"SELECT id AS "id!" FROM users WHERE email = $1"#,
    email
  )
  .fetch_optional(pool)
  .await?
  .is_some()
  {
    return Err(bad_request("user already exists").into());
  }

  let password_hash = hash_password(&password)?;
  let user_id = Uuid::now_v7();
  let account_id = Uuid::now_v7();

  // requireEmailVerification=false (non-prod Better Auth behavior): auto sign-in.
  sqlx::query!(
        r#"
        INSERT INTO users (id, name, email, email_verified, role, created_at, updated_at)
        VALUES ($1, $2, $3, false, 'user', now(), now())
        "#,
        user_id,
        name,
        email,
    )
    .execute(pool)
    .await?;

  sqlx::query!(
        r#"
        INSERT INTO accounts (id, account_id, provider_id, user_id, password, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, now(), now())
        "#,
        account_id,
        user_id.to_string(),
        CREDENTIAL_PROVIDER_ID,
        user_id,
        password_hash,
    )
    .execute(pool)
    .await?;

  let session = session::start(cx).await?;
  let (ip, ua) = client_meta(cx);
  let session_id = session_store::insert_session(
    pool,
    &session,
    user_id,
    ip.as_deref(),
    ua.as_deref(),
    None,
  )
  .await?;

  let payload = SessionPayload::from(&SessionUser {
    user: User {
      id: user_id,
      name,
      email,
      email_verified: false,
      image: None,
      two_factor_enabled: false,
      role: Some("user".into()),
      banned: false,
      ban_reason: None,
      ban_expires: None,
    },
    session_id,
    expires_at: system_time_to_primitive(session.expires_at),
    impersonated_by: None,
  });

  Ok((StatusCode::OK, Json(payload)))
}
