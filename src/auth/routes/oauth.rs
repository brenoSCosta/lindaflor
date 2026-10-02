//! Google OAuth callback: GET `/api/auth/callback/google`

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    error::{bad_request, redirect, unauthorized},
    query_params, route,
  },
  session,
};
use uuid::Uuid;

use crate::auth::google::{
  GOOGLE_PROVIDER_ID, GoogleOAuthConfig, exchange_code, fetch_userinfo,
  safe_callback_path,
};
use crate::auth::routes::dto::{client_meta, normalize_email};
use crate::auth::session_store;

const OAUTH_STATE_PREFIX: &str = "oauth-google:";

/// OAuth CSRF state lifetime (10 minutes).
pub const OAUTH_STATE_TTL_SECS: i64 = 10 * 60;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthStateValue {
  callback: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  link_user_id: Option<Uuid>,
}

#[derive(Debug)]
pub struct ConsumedOAuthState {
  pub callback_path: String,
  pub link_user_id: Option<Uuid>,
}

#[derive(Debug)]
#[query_params(error = bad_request)]
struct GoogleCallbackQuery {
  code: Option<String>,
  state: Option<String>,
  error: Option<String>,
  error_description: Option<String>,
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

fn expires_at_from_secs(secs: i64) -> PrimitiveDateTime {
  let odt = time::OffsetDateTime::now_utc() + time::Duration::seconds(secs);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

/// Persist OAuth `state` → callback path (and optional link-user intent) in `verifications`.
pub async fn store_oauth_state(
  pool: &PgPool,
  state: &str,
  callback_path: &str,
  ttl_secs: i64,
  link_user_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
  let id = Uuid::now_v7();
  let identifier = format!("{OAUTH_STATE_PREFIX}{state}");
  let expires_at = expires_at_from_secs(ttl_secs);
  let value = serde_json::to_string(&OAuthStateValue {
    callback: callback_path.to_string(),
    link_user_id,
  })
  .unwrap_or_else(|_| callback_path.to_string());
  sqlx::query!(
        r#"
        INSERT INTO verifications (id, identifier, value, expires_at, created_at, updated_at)
        VALUES ($1, $2, $3, $4, now(), now())
        "#,
        id,
        identifier,
        value,
        expires_at,
    )
    .execute(pool)
    .await?;
  Ok(())
}

async fn consume_oauth_state(
  pool: &PgPool,
  state: &str,
) -> Result<ConsumedOAuthState, topcoat::Error> {
  let identifier = format!("{OAUTH_STATE_PREFIX}{state}");
  let row = sqlx::query!(
    r#"
        SELECT id, value
        FROM verifications
        WHERE identifier = $1
          AND expires_at > now()
        "#,
    identifier,
  )
  .fetch_optional(pool)
  .await?;

  let Some(row) = row else {
    return Err(bad_request("invalid or expired OAuth state").into());
  };

  sqlx::query!("DELETE FROM verifications WHERE id = $1", row.id)
    .execute(pool)
    .await?;

  let parsed =
    if let Ok(v) = serde_json::from_str::<OAuthStateValue>(&row.value) {
      ConsumedOAuthState {
        callback_path: safe_callback_path(Some(&v.callback)),
        link_user_id: v.link_user_id,
      }
    } else {
      // Legacy plain callback path.
      ConsumedOAuthState {
        callback_path: safe_callback_path(Some(&row.value)),
        link_user_id: None,
      }
    };

  Ok(parsed)
}

#[utoipa::path(
    get,
    path = "/api/auth/callback/google",
    tag = "auth",
    params(
        ("code" = Option<String>, Query, description = "Authorization code from Google"),
        ("state" = Option<String>, Query, description = "CSRF state token"),
        ("error" = Option<String>, Query, description = "OAuth error code from Google"),
        ("error_description" = Option<String>, Query, description = "OAuth error detail")
    ),
    responses(
        (status = 307, description = "Signed in; redirect to callbackURL"),
        (status = 400, description = "Missing/invalid OAuth parameters or state")
    )
)]
#[route(GET "/api/auth/callback/google")]
pub async fn google_callback(cx: &Cx) -> Result<()> {
  let query = query_params::<GoogleCallbackQuery>(cx)?;

  if let Some(err) = query.error.as_deref().filter(|e| !e.is_empty()) {
    tracing::warn!(
        error = err,
        description = ?query.error_description,
        "Google OAuth denied or failed"
    );
    return Err(redirect("/login").into());
  }

  let Some(code) = query
    .code
    .as_deref()
    .map(str::trim)
    .filter(|c| !c.is_empty())
  else {
    return Err(bad_request("code is required").into());
  };
  let Some(state) = query
    .state
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
  else {
    return Err(bad_request("state is required").into());
  };

  let Some(config) = GoogleOAuthConfig::from_env() else {
    return Err(bad_request(
            "Google sign-in is not configured (set GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET, and APP_ORIGIN)",
        )
        .into());
  };

  let pool = app_context::<PgPool>(cx);
  let oauth_state = consume_oauth_state(pool, state).await?;
  let callback_path = oauth_state.callback_path;

  let tokens = exchange_code(&config, code).await.map_err(|e| {
    tracing::error!(error = %e, "Google token exchange failed");
    bad_request("Google token exchange failed")
  })?;

  let profile = fetch_userinfo(&tokens.access_token).await.map_err(|e| {
    tracing::error!(error = %e, "Google userinfo failed");
    bad_request("Google userinfo failed")
  })?;

  let email = profile
    .email
    .as_deref()
    .map(normalize_email)
    .filter(|e| e.contains('@'))
    .ok_or_else(|| bad_request("Google account has no email"))?;

  let name = profile
    .name
    .as_deref()
    .map(str::trim)
    .filter(|n| !n.is_empty())
    .unwrap_or(email.split('@').next().unwrap_or("user"))
    .to_string();
  let image = profile
    .picture
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned);
  let email_verified = profile.email_verified.unwrap_or(true);
  let google_sub = profile.sub;

  let access_expires = tokens.expires_in.map(expires_at_from_secs);

  let user_id = if let Some(link_user_id) = oauth_state.link_user_id {
    link_google_account(
      pool,
      link_user_id,
      &google_sub,
      image.as_deref(),
      email_verified,
      &tokens.access_token,
      tokens.refresh_token.as_deref(),
      tokens.id_token.as_deref(),
      tokens.scope.as_deref(),
      access_expires,
    )
    .await?
  } else {
    find_or_create_google_user(
      pool,
      &google_sub,
      &email,
      &name,
      image.as_deref(),
      email_verified,
      &tokens.access_token,
      tokens.refresh_token.as_deref(),
      tokens.id_token.as_deref(),
      tokens.scope.as_deref(),
      access_expires,
    )
    .await?
  };

  let user_row = sqlx::query!(
    r#"
        SELECT id, banned, ban_expires
        FROM users
        WHERE id = $1
        "#,
    user_id,
  )
  .fetch_one(pool)
  .await?;

  if is_currently_banned(user_row.banned, user_row.ban_expires) {
    return Err(unauthorized().into());
  }

  let session = session::start(cx).await?;
  let (ip, ua) = client_meta(cx);
  session_store::insert_session(
    pool,
    &session,
    user_id,
    ip.as_deref(),
    ua.as_deref(),
    None,
  )
  .await?;

  Err(redirect(callback_path).into())
}

/// Link a Google account to an already-authenticated user (link-social flow).
#[allow(clippy::too_many_arguments)]
async fn link_google_account(
  pool: &PgPool,
  link_user_id: Uuid,
  google_sub: &str,
  image: Option<&str>,
  email_verified: bool,
  access_token: &str,
  refresh_token: Option<&str>,
  id_token: Option<&str>,
  scope: Option<&str>,
  access_expires: Option<PrimitiveDateTime>,
) -> Result<Uuid> {
  let user_exists =
    sqlx::query!("SELECT id FROM users WHERE id = $1", link_user_id)
      .fetch_optional(pool)
      .await?;
  if user_exists.is_none() {
    return Err(bad_request("user no longer exists").into());
  }

  if let Some(existing) = sqlx::query!(
    r#"
        SELECT user_id
        FROM accounts
        WHERE provider_id = $1 AND account_id = $2
        "#,
    GOOGLE_PROVIDER_ID,
    google_sub,
  )
  .fetch_optional(pool)
  .await?
  {
    if existing.user_id != link_user_id {
      return Err(
        bad_request("this Google account is already linked to another user")
          .into(),
      );
    }

    sqlx::query!(
      r#"
            UPDATE accounts
            SET
                access_token = $3,
                refresh_token = COALESCE($4, refresh_token),
                id_token = COALESCE($5, id_token),
                scope = COALESCE($6, scope),
                access_token_expires_at = $7,
                updated_at = now()
            WHERE provider_id = $1 AND account_id = $2
            "#,
      GOOGLE_PROVIDER_ID,
      google_sub,
      access_token,
      refresh_token,
      id_token,
      scope,
      access_expires,
    )
    .execute(pool)
    .await?;
  } else {
    let account_id = Uuid::now_v7();
    sqlx::query!(
      r#"
            INSERT INTO accounts (
                id, account_id, provider_id, user_id,
                access_token, refresh_token, id_token, scope,
                access_token_expires_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), now())
            "#,
      account_id,
      google_sub,
      GOOGLE_PROVIDER_ID,
      link_user_id,
      access_token,
      refresh_token,
      id_token,
      scope,
      access_expires,
    )
    .execute(pool)
    .await?;
  }

  if email_verified || image.is_some() {
    sqlx::query!(
      r#"
            UPDATE users
            SET
                email_verified = CASE WHEN $2 THEN true ELSE email_verified END,
                image = COALESCE(image, $3),
                updated_at = now()
            WHERE id = $1
            "#,
      link_user_id,
      email_verified,
      image,
    )
    .execute(pool)
    .await?;
  }

  Ok(link_user_id)
}

#[allow(clippy::too_many_arguments)]
async fn find_or_create_google_user(
  pool: &PgPool,
  google_sub: &str,
  email: &str,
  name: &str,
  image: Option<&str>,
  email_verified: bool,
  access_token: &str,
  refresh_token: Option<&str>,
  id_token: Option<&str>,
  scope: Option<&str>,
  access_expires: Option<PrimitiveDateTime>,
) -> Result<Uuid> {
  // Existing Google account linkage.
  if let Some(existing) = sqlx::query!(
    r#"
        SELECT user_id
        FROM accounts
        WHERE provider_id = $1 AND account_id = $2
        "#,
    GOOGLE_PROVIDER_ID,
    google_sub,
  )
  .fetch_optional(pool)
  .await?
  {
    sqlx::query!(
      r#"
            UPDATE accounts
            SET
                access_token = $3,
                refresh_token = COALESCE($4, refresh_token),
                id_token = COALESCE($5, id_token),
                scope = COALESCE($6, scope),
                access_token_expires_at = $7,
                updated_at = now()
            WHERE provider_id = $1 AND account_id = $2
            "#,
      GOOGLE_PROVIDER_ID,
      google_sub,
      access_token,
      refresh_token,
      id_token,
      scope,
      access_expires,
    )
    .execute(pool)
    .await?;

    if let Some(img) = image {
      sqlx::query!(
        r#"
                UPDATE users
                SET image = COALESCE(image, $2), updated_at = now()
                WHERE id = $1
                "#,
        existing.user_id,
        img,
      )
      .execute(pool)
      .await?;
    }

    return Ok(existing.user_id);
  }

  // Link to existing user by email, or create a new user.
  let user_id = if let Some(existing_user) =
    sqlx::query!(r#"SELECT id FROM users WHERE email = $1"#, email,)
      .fetch_optional(pool)
      .await?
  {
    if email_verified {
      sqlx::query!(
        r#"
                UPDATE users
                SET
                    email_verified = true,
                    image = COALESCE(image, $2),
                    updated_at = now()
                WHERE id = $1
                "#,
        existing_user.id,
        image,
      )
      .execute(pool)
      .await?;
    } else if image.is_some() {
      sqlx::query!(
        r#"
                UPDATE users
                SET image = COALESCE(image, $2), updated_at = now()
                WHERE id = $1
                "#,
        existing_user.id,
        image,
      )
      .execute(pool)
      .await?;
    }
    existing_user.id
  } else {
    let user_id = Uuid::now_v7();
    sqlx::query!(
            r#"
            INSERT INTO users (
                id, name, email, email_verified, image, role, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, 'user', now(), now())
            "#,
            user_id,
            name,
            email,
            email_verified,
            image,
        )
        .execute(pool)
        .await?;
    user_id
  };

  let account_id = Uuid::now_v7();
  sqlx::query!(
    r#"
        INSERT INTO accounts (
            id, account_id, provider_id, user_id,
            access_token, refresh_token, id_token, scope,
            access_token_expires_at, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), now())
        "#,
    account_id,
    google_sub,
    GOOGLE_PROVIDER_ID,
    user_id,
    access_token,
    refresh_token,
    id_token,
    scope,
    access_expires,
  )
  .execute(pool)
  .await?;

  Ok(user_id)
}
