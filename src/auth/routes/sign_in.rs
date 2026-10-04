use std::time::{SystemTime, UNIX_EPOCH};

use http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Json,
    error::{bad_request, unauthorized},
    route,
  },
  session,
};

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::verify_password;
use crate::auth::routes::dto::{
  SessionPayload, client_meta, normalize_email, random_token,
};
use crate::auth::routes::two_factor::create_pending_2fa;
use crate::auth::session_store;
use crate::auth::user::{SessionUser, User};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInEmailBody {
  pub email: String,
  pub password: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TwoFactorRequiredBody {
  pub code: &'static str,
  pub message: &'static str,
  pub two_factor_redirect: bool,
  /// Short-lived token for `/api/auth/two-factor/verify-totp` / `verify-backup-code`.
  pub token: String,
}

fn system_time_to_primitive(st: SystemTime) -> PrimitiveDateTime {
  let duration = st.duration_since(UNIX_EPOCH).unwrap_or_default();
  let odt =
    time::OffsetDateTime::from_unix_timestamp(duration.as_secs() as i64)
      .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

fn is_currently_banned(
  banned: bool,
  ban_expires: Option<PrimitiveDateTime>,
) -> bool {
  if !banned {
    return false;
  }
  match ban_expires {
    None => true,
    Some(expires) => {
      let now = time::OffsetDateTime::now_utc();
      expires.assume_utc() > now
    }
  }
}

#[utoipa::path(
    post,
    path = "/api/auth/sign-in/email",
    tag = "auth",
    request_body = SignInEmailBody,
    responses(
        (status = 200, description = "Signed in", body = SessionPayload),
        (status = 403, description = "Two-factor authentication required", body = TwoFactorRequiredBody)
    )
)]
#[route(POST "/api/auth/sign-in/email")]
pub async fn sign_in_email(
  cx: &Cx,
  Json(body): Json<SignInEmailBody>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
  let email = normalize_email(&body.email);
  if email.is_empty() || body.password.is_empty() {
    return Err(bad_request("email and password are required").into());
  }

  let pool = app_context::<PgPool>(cx);

  let row = sqlx::query!(
    r#"
        SELECT
            u.id,
            u.name,
            u.email,
            u.email_verified,
            u.image,
            u.two_factor_enabled,
            u.role,
            u.banned,
            u.ban_reason,
            u.ban_expires,
            a.password
        FROM users u
        INNER JOIN accounts a ON a.user_id = u.id AND a.provider_id = $2
        WHERE u.email = $1
        "#,
    email,
    CREDENTIAL_PROVIDER_ID,
  )
  .fetch_optional(pool)
  .await?;

  let Some(row) = row else {
    return Err(unauthorized().into());
  };

  let Some(stored_hash) = row.password.as_deref() else {
    return Err(unauthorized().into());
  };

  if !verify_password(&body.password, stored_hash)? {
    return Err(unauthorized().into());
  }

  if is_currently_banned(row.banned, row.ban_expires) {
    return Err(unauthorized().into());
  }

  if row.two_factor_enabled {
    let token = create_pending_2fa(pool, row.id).await?;
    return Ok((
      StatusCode::FORBIDDEN,
      Json(serde_json::to_value(TwoFactorRequiredBody {
        code: "TWO_FACTOR_REQUIRED",
        message: "Two-factor authentication is required",
        two_factor_redirect: true,
        token,
      })?),
    ));
  }

  let session = session::start(cx).await?;
  let (ip, ua) = client_meta(cx);
  let session_id = session_store::insert_session(
    pool,
    &session,
    row.id,
    ip.as_deref(),
    ua.as_deref(),
    None,
  )
  .await?;

  let payload = SessionPayload::from(&SessionUser {
    user: User {
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
    },
    session_id,
    expires_at: system_time_to_primitive(session.expires_at),
    impersonated_by: None,
  });

  Ok((StatusCode::OK, Json(serde_json::to_value(payload)?)))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInSocialBody {
  pub provider: String,
  pub callback_url: Option<String>,
  pub error_callback_url: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInSocialResponse {
  /// Google authorize URL for the client to navigate to.
  pub url: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SocialNotImplementedBody {
  pub code: &'static str,
  pub message: String,
}

/// Start Google OAuth. Returns `{ url }` for the client to redirect.
#[utoipa::path(
    post,
    path = "/api/auth/sign-in/social",
    tag = "auth",
    request_body = SignInSocialBody,
    responses(
        (status = 200, description = "OAuth authorize URL", body = SignInSocialResponse),
        (status = 400, description = "Unsupported provider", body = SocialNotImplementedBody),
        (status = 501, description = "Social sign-in not configured", body = SocialNotImplementedBody)
    )
)]
#[route(POST "/api/auth/sign-in/social")]
pub async fn sign_in_social(
  cx: &Cx,
  Json(body): Json<SignInSocialBody>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
  let provider = body.provider.trim().to_ascii_lowercase();
  if provider != "google" {
    return Ok((
      StatusCode::BAD_REQUEST,
      Json(serde_json::to_value(SocialNotImplementedBody {
        code: "SOCIAL_SIGN_IN_UNAVAILABLE",
        message: format!("social provider '{provider}' is not supported"),
      })?),
    ));
  }

  let Some(config) = crate::auth::google::GoogleOAuthConfig::from_env() else {
    let has_google_creds = std::env::var("GOOGLE_CLIENT_ID")
      .map(|v| !v.trim().is_empty())
      .unwrap_or(false)
      && std::env::var("GOOGLE_CLIENT_SECRET")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let message = if has_google_creds {
      "Google sign-in is not configured (set APP_ORIGIN, WEB_ORIGIN, or BETTER_AUTH_URL)"
                .to_string()
    } else {
      "Google sign-in is not configured (set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET)"
                .to_string()
    };
    return Ok((
      StatusCode::NOT_IMPLEMENTED,
      Json(serde_json::to_value(SocialNotImplementedBody {
        code: "SOCIAL_SIGN_IN_UNAVAILABLE",
        message,
      })?),
    ));
  };

  let _ = body.error_callback_url;
  let callback_path =
    crate::auth::google::safe_callback_path(body.callback_url.as_deref());
  let state = random_token();
  let pool = app_context::<PgPool>(cx);
  crate::auth::routes::oauth::store_oauth_state(
    pool,
    &state,
    &callback_path,
    crate::auth::routes::oauth::OAUTH_STATE_TTL_SECS,
    None,
  )
  .await?;

  let url = config.authorize_url(&state);
  Ok((
    StatusCode::OK,
    Json(serde_json::to_value(SignInSocialResponse { url })?),
  ))
}
