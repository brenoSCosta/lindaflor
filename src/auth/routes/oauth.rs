use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    error::{bad_request, redirect, unauthorized},
    href, query_params, route,
  },
  session,
};
use uuid::Uuid;

use crate::auth::google::{
  GOOGLE_PROVIDER_ID, GoogleOAuthConfig, GoogleOAuthError, GoogleTokenResponse,
  fetch_userinfo, safe_callback_path,
};
use crate::auth::routes::dto::{client_meta, normalize_email};
use crate::auth::session_store;
use crate::auth::user::{current_user_owned, is_currently_banned};

const OAUTH_STATE_PREFIX: &str = "oauth-google:";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// OAuth CSRF state lifetime (10 minutes).
pub const OAUTH_STATE_TTL_SECS: i64 = 10 * 60;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthStateValue {
  callback: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  link_user_id: Option<Uuid>,
  /// Hex session-token hash (link flow) the callback must present.
  /// `None` for legacy login-flow rows written before session binding.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  session_binding: Option<String>,
  /// PKCE `code_verifier`; sent as `code_verifier` at token exchange.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  code_verifier: Option<String>,
  /// OIDC `nonce` sent in the authorize URL (stored server-side).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  nonce: Option<String>,
}

#[derive(Debug)]
pub struct ConsumedOAuthState {
  pub callback_path: String,
  pub link_user_id: Option<Uuid>,
  pub session_binding: Option<String>,
  pub code_verifier: Option<String>,
  pub nonce: Option<String>,
}

#[derive(Debug)]
#[query_params(error = bad_request)]
struct GoogleCallbackQuery {
  code: Option<String>,
  state: Option<String>,
  error: Option<String>,
  error_description: Option<String>,
}

fn expires_at_from_secs(secs: i64) -> PrimitiveDateTime {
  let odt = time::OffsetDateTime::now_utc() + time::Duration::seconds(secs);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

/// Google's `email_verified` claim gates account linking.
///
/// Missing (`None`) and `false` are both unverified: never default to `true`.
pub fn google_email_verified(claim: Option<bool>) -> bool {
  claim == Some(true)
}

const UNVERIFIED_GOOGLE_EMAIL_MSG: &str =
  "please verify your Google email first, then try again";

/// Whether a verified Google email should link an existing row or insert one.
///
/// Unverified (`false`/`None`) aborts both linking *and* create so a
/// later unique-email collision cannot take over the victim account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoogleEmailSignupAction {
  LinkExisting,
  CreateNew,
}

pub fn google_email_signup_action(
  email_verified: bool,
  existing_user_by_email: bool,
) -> Result<GoogleEmailSignupAction, &'static str> {
  if !email_verified {
    return Err(UNVERIFIED_GOOGLE_EMAIL_MSG);
  }
  if existing_user_by_email {
    Ok(GoogleEmailSignupAction::LinkExisting)
  } else {
    Ok(GoogleEmailSignupAction::CreateNew)
  }
}

/// Base64url (no padding) encoding for PKCE S256 challenges.
fn base64url_nopad(input: &[u8]) -> String {
  const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
  let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
  for chunk in input.chunks(3) {
    let b0 = chunk[0] as u32;
    let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
    let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
    let n = (b0 << 16) | (b1 << 8) | b2;
    out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
    out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
    if chunk.len() > 1 {
      out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
    }
    if chunk.len() > 2 {
      out.push(ALPHABET[(n & 63) as usize] as char);
    }
  }
  out
}

/// PKCE S256 `code_challenge` for a `code_verifier` (RFC 7636 §4.2).
pub fn pkce_s256_challenge(verifier: &str) -> String {
  use sha2::{Digest, Sha256};
  base64url_nopad(&Sha256::digest(verifier.as_bytes()))
}

/// Authorize URL with optional PKCE `code_challenge` (S256) and OIDC `nonce`.
pub fn google_authorize_url(
  config: &GoogleOAuthConfig,
  state: &str,
  code_challenge: Option<&str>,
  nonce: Option<&str>,
) -> String {
  config.authorize_url_with(state, code_challenge, nonce)
}

/// Verify a link-flow callback runs in the session that started it.
///
/// `expected_binding` is the hex session-token hash stored server-side by
/// [`store_oauth_state_full`]; `actual_*` describe the current request.
/// Rejects cross-session completion (link CSRF) before any Google calls.
pub fn verify_link_session(
  expected_binding: Option<&str>,
  actual_hash_hex: Option<&str>,
  link_user_id: Uuid,
  actual_user_id: Option<Uuid>,
) -> Result<(), &'static str> {
  let Some(expected) =
    expected_binding.map(str::trim).filter(|s| !s.is_empty())
  else {
    return Err("OAuth state is not bound to a session; restart linking");
  };
  let Some(actual) = actual_hash_hex.map(str::trim).filter(|s| !s.is_empty())
  else {
    return Err("OAuth session changed during linking; sign in and try again");
  };
  if expected != actual {
    return Err("OAuth session changed during linking; sign in and try again");
  }
  if actual_user_id != Some(link_user_id) {
    return Err("OAuth link account mismatch; sign in and try again");
  }
  Ok(())
}

/// Persist OAuth `state` → callback path (and optional link-user intent) in `verifications`.
pub async fn store_oauth_state(
  pool: &PgPool,
  state: &str,
  callback_path: &str,
  ttl_secs: i64,
  link_user_id: Option<Uuid>,
) -> Result<(), sqlx::Error> {
  store_oauth_state_full(
    pool,
    NewOAuthState {
      state,
      callback_path,
      ttl_secs,
      link_user_id,
      session_binding: None,
      code_verifier: None,
      nonce: None,
    },
  )
  .await
}

/// Persist OAuth `state` with server-side session binding + PKCE (link flow).
///
/// `session_binding` is the hex session-token hash that must present the
/// `state` on callback; `link_user_id` never leaves the server (it is not
/// embedded in the `state` string handed to the browser).
#[derive(Debug)]
pub struct NewOAuthState<'a> {
  pub state: &'a str,
  pub callback_path: &'a str,
  pub ttl_secs: i64,
  pub link_user_id: Option<Uuid>,
  pub session_binding: Option<&'a str>,
  pub code_verifier: Option<&'a str>,
  pub nonce: Option<&'a str>,
}

pub async fn store_oauth_state_full(
  pool: &PgPool,
  params: NewOAuthState<'_>,
) -> Result<(), sqlx::Error> {
  let NewOAuthState {
    state,
    callback_path,
    ttl_secs,
    link_user_id,
    session_binding,
    code_verifier,
    nonce,
  } = params;
  let id = Uuid::now_v7();
  let identifier = format!("{OAUTH_STATE_PREFIX}{state}");
  let expires_at = expires_at_from_secs(ttl_secs);
  let value = serde_json::to_string(&OAuthStateValue {
    callback: callback_path.to_string(),
    link_user_id,
    session_binding: session_binding.map(str::to_owned),
    code_verifier: code_verifier.map(str::to_owned),
    nonce: nonce.map(str::to_owned),
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

pub async fn consume_oauth_state(
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
        session_binding: v.session_binding,
        code_verifier: v.code_verifier,
        nonce: v.nonce,
      }
    } else {
      // Legacy plain callback path.
      ConsumedOAuthState {
        callback_path: safe_callback_path(Some(&row.value)),
        link_user_id: None,
        session_binding: None,
        code_verifier: None,
        nonce: None,
      }
    };

  Ok(parsed)
}

/// Exchange an authorization code for tokens, presenting the PKCE
/// `code_verifier` stored server-side when the flow started with a
/// `code_challenge` (mirrors `google::exchange_code` + `code_verifier`).
async fn exchange_code_with_verifier(
  config: &GoogleOAuthConfig,
  code: &str,
  code_verifier: &str,
) -> Result<GoogleTokenResponse, GoogleOAuthError> {
  let redirect_uri = config.redirect_uri();
  let client = reqwest::Client::new();
  let response = client
    .post(GOOGLE_TOKEN_URL)
    .header("Accept", "application/json")
    .form(&[
      ("code", code),
      ("client_id", config.client_id.as_str()),
      ("client_secret", config.client_secret.as_str()),
      ("redirect_uri", redirect_uri.as_str()),
      ("grant_type", "authorization_code"),
      ("code_verifier", code_verifier),
    ])
    .send()
    .await?;

  let status = response.status();
  let body = response.text().await?;
  if !status.is_success() {
    return Err(GoogleOAuthError::Token(format!("status {status}: {body}")));
  }

  serde_json::from_str(&body)
    .map_err(|e| GoogleOAuthError::Token(e.to_string()))
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
    return Err(redirect(href!(crate::app::login::page).resolve(cx)).into());
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
  let callback_path = oauth_state.callback_path.clone();

  // A link flow must complete in the session that started it.
  // Verify before any Google calls so a forged cross-session callback
  // fails without network access.
  if let Some(link_user_id) = oauth_state.link_user_id {
    let current_hash_hex = session::token_hash(cx)
      .await?
      .map(|hash| session_store::token_hash_hex(&hash));
    let current_user_id = current_user_owned(cx).await?.map(|su| su.user.id);
    if let Err(message) = verify_link_session(
      oauth_state.session_binding.as_deref(),
      current_hash_hex.as_deref(),
      link_user_id,
      current_user_id,
    ) {
      return Err(bad_request(message).into());
    }
  }

  // `nonce` is generated, stored, and sent on the authorize
  // URL. We do not parse Google's `id_token` JWT to check the nonce claim
  // (`jsonwebtoken` is not a dependency). PKCE + random `state` still bind
  // the authorization code to this start.
  let _stored_nonce = oauth_state.nonce.as_deref();

  let Some(verifier) = oauth_state
    .code_verifier
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
  else {
    return Err(
      bad_request("OAuth PKCE verifier missing; restart sign-in").into(),
    );
  };
  let tokens = exchange_code_with_verifier(&config, code, verifier)
    .await
    .map_err(|e| {
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
  // A missing claim is unverified — never default to `true`.
  let email_verified = google_email_verified(profile.email_verified);
  let google_sub = profile.sub;

  let access_expires = tokens.expires_in.map(expires_at_from_secs);

  let account = GoogleAccountUpsert {
    google_sub: &google_sub,
    image: image.as_deref(),
    email_verified,
    access_token: &tokens.access_token,
    refresh_token: tokens.refresh_token.as_deref(),
    id_token: tokens.id_token.as_deref(),
    scope: tokens.scope.as_deref(),
    access_expires,
  };

  let user_id = if let Some(link_user_id) = oauth_state.link_user_id {
    link_google_account(pool, link_user_id, account).await?
  } else {
    find_or_create_google_user(pool, &email, &name, account).await?
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

/// Grouped Google account upsert data to keep fn signatures small.
#[derive(Debug, Clone, Copy)]
pub struct GoogleAccountUpsert<'a> {
  pub google_sub: &'a str,
  pub image: Option<&'a str>,
  pub email_verified: bool,
  pub access_token: &'a str,
  pub refresh_token: Option<&'a str>,
  pub id_token: Option<&'a str>,
  pub scope: Option<&'a str>,
  pub access_expires: Option<PrimitiveDateTime>,
}

/// Link a Google account to an already-authenticated user (link-social flow).
async fn link_google_account(
  pool: &PgPool,
  link_user_id: Uuid,
  account: GoogleAccountUpsert<'_>,
) -> Result<Uuid> {
  let image = account.image;
  let email_verified = account.email_verified;
  let google_sub = account.google_sub;
  let access_token = account.access_token;
  let refresh_token = account.refresh_token;
  let id_token = account.id_token;
  let scope = account.scope;
  let access_expires = account.access_expires;
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

pub async fn find_or_create_google_user(
  pool: &PgPool,
  email: &str,
  name: &str,
  account: GoogleAccountUpsert<'_>,
) -> Result<Uuid> {
  let google_sub = account.google_sub;
  let image = account.image;
  let email_verified = account.email_verified;
  let access_token = account.access_token;
  let refresh_token = account.refresh_token;
  let id_token = account.id_token;
  let scope = account.scope;
  let access_expires = account.access_expires;
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
  //
  // Both linking and first-time create require a verified Google
  // email. An unverified (`false`/`None`) claim aborts: it must not merge
  // into the victim row, must not flip `users.email_verified` to true, and
  // must not create a second row that later collides on the unique email.
  let existing_user =
    sqlx::query!(r#"SELECT id FROM users WHERE email = $1"#, email,)
      .fetch_optional(pool)
      .await?;
  let user_id = match google_email_signup_action(
    email_verified,
    existing_user.is_some(),
  ) {
    Err(message) => return Err(bad_request(message).into()),
    Ok(GoogleEmailSignupAction::LinkExisting) => {
      let existing_user = existing_user.expect("existing email row");
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
      existing_user.id
    }
    Ok(GoogleEmailSignupAction::CreateNew) => {
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
    }
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn missing_email_verified_claim_is_not_verified() {
    // `None` must never default to `true`.
    assert!(!google_email_verified(None));
    assert!(!google_email_verified(Some(false)));
    assert!(google_email_verified(Some(true)));
  }

  #[test]
  fn unverified_claim_does_not_link_or_create() {
    assert_eq!(
      google_email_signup_action(false, true).unwrap_err(),
      UNVERIFIED_GOOGLE_EMAIL_MSG
    );
    assert_eq!(
      google_email_signup_action(false, false).unwrap_err(),
      UNVERIFIED_GOOGLE_EMAIL_MSG
    );
    assert_eq!(
      google_email_signup_action(google_email_verified(None), true)
        .unwrap_err(),
      UNVERIFIED_GOOGLE_EMAIL_MSG
    );
    assert_eq!(
      google_email_signup_action(google_email_verified(None), false)
        .unwrap_err(),
      UNVERIFIED_GOOGLE_EMAIL_MSG
    );
  }

  #[test]
  fn verified_claim_links_or_creates() {
    assert_eq!(
      google_email_signup_action(true, true).unwrap(),
      GoogleEmailSignupAction::LinkExisting
    );
    assert_eq!(
      google_email_signup_action(true, false).unwrap(),
      GoogleEmailSignupAction::CreateNew
    );
    assert_eq!(
      google_email_signup_action(google_email_verified(Some(true)), true)
        .unwrap(),
      GoogleEmailSignupAction::LinkExisting
    );
  }

  #[test]
  fn pkce_challenge_matches_rfc7636_vector() {
    // RFC 7636 §4.2 test vector.
    assert_eq!(
      pkce_s256_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
      "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    // Verifier-length challenges stay URL-safe (no `+`, `/`, `=`).
    for verifier in ["a", "ab", "abc", "abcd"] {
      let challenge = pkce_s256_challenge(verifier);
      assert!(
        challenge
          .chars()
          .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "{challenge}"
      );
    }
  }

  #[test]
  fn authorize_url_carries_pkce_and_nonce() {
    let config = GoogleOAuthConfig {
      client_id: "fake-client.apps.googleusercontent.com".into(),
      client_secret: "GOCSPX-fake".into(),
      app_origin: "http://localhost:4200".into(),
    };
    let url = google_authorize_url(
      &config,
      "state-1",
      Some("challenge-1"),
      Some("n-1"),
    );
    assert!(url.contains("state=state-1"));
    assert!(url.contains("code_challenge=challenge-1"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("nonce=n-1"));

    let plain = google_authorize_url(&config, "state-1", None, None);
    assert!(!plain.contains("code_challenge"));
    assert!(!plain.contains("nonce="));
  }

  #[test]
  fn link_session_binding_accepts_only_matching_session_and_user() {
    let link_user = Uuid::now_v7();
    let other_user = Uuid::now_v7();

    // Happy path: same session hash, same user.
    assert!(
      verify_link_session(
        Some("hash-a"),
        Some("hash-a"),
        link_user,
        Some(link_user)
      )
      .is_ok()
    );

    // Cross-session completion (link CSRF) is rejected.
    assert!(
      verify_link_session(
        Some("hash-a"),
        Some("hash-b"),
        link_user,
        Some(link_user)
      )
      .is_err()
    );

    // Missing binding (legacy row) or missing session is rejected.
    assert!(
      verify_link_session(None, Some("hash-a"), link_user, Some(link_user))
        .is_err()
    );
    assert!(
      verify_link_session(Some("hash-a"), None, link_user, Some(link_user))
        .is_err()
    );
    assert!(
      verify_link_session(Some("hash-a"), Some(""), link_user, Some(link_user))
        .is_err()
    );

    // Right session but different user is rejected.
    assert!(
      verify_link_session(
        Some("hash-a"),
        Some("hash-a"),
        link_user,
        Some(other_user)
      )
      .is_err()
    );
    assert!(
      verify_link_session(Some("hash-a"), Some("hash-a"), link_user, None)
        .is_err()
    );
  }
}
