use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::{
  Result,
  context::Cx,
  cookie::{Cookie, Cookies, SameSite, cookies},
  router::error::{bad_request, forbidden, unauthorized},
  session,
};
use uuid::Uuid;

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::google::{GoogleOAuthConfig, safe_callback_path};
use crate::auth::password::{
  MAX_PASSWORD_LEN, MIN_PASSWORD_LEN, hash_password, validate_password,
  verify_password,
};
use crate::auth::routes::dto::{
  client_meta, format_primitive, normalize_email, random_token,
};
use crate::auth::routes::oauth::{
  NewOAuthState, OAUTH_STATE_TTL_SECS, google_authorize_url,
  pkce_s256_challenge, store_oauth_state_full,
};
use crate::auth::routes::two_factor::{
  PENDING_2FA_PREFIX, create_pending_2fa, guard_pending_2fa,
};
use crate::auth::session_store::{self, token_hash_hex};
use crate::auth::totp::{
  consume_backup_code, generate_backup_codes, generate_secret,
  hash_backup_codes, verify_code,
};
use crate::auth::totp_crypto::{decrypt_row_secret, encrypt_secret};
use crate::auth::user::{SessionUser, User, is_currently_banned};

pub const PENDING_2FA_COOKIE: &str = "lindaflor_pending_2fa";
pub const PENDING_2FA_COOKIE_HOST: &str = "__Host-lindaflor_pending_2fa";

fn pending_2fa_cookie_secure() -> bool {
  !matches!(crate::config::app_env().as_str(), "development" | "dev")
}

fn pending_2fa_cookie_name() -> &'static str {
  if pending_2fa_cookie_secure() {
    PENDING_2FA_COOKIE_HOST
  } else {
    PENDING_2FA_COOKIE
  }
}

const RESET_TOKEN_TTL_SECS: i64 = 60 * 60;
const RESET_IDENTIFIER_PREFIX: &str = "reset-password:";
const VERIFY_TOKEN_TTL_SECS: i64 = 24 * 60 * 60;
const VERIFY_IDENTIFIER_PREFIX: &str = "email-verification:";
const CHANGE_EMAIL_TOKEN_TTL_SECS: i64 = 30 * 60;
const CHANGE_EMAIL_IDENTIFIER_PREFIX: &str = "change-email:";
const DELETE_TOKEN_TTL_SECS: i64 = 30 * 60;
const DELETE_IDENTIFIER_PREFIX: &str = "delete-user:";

/// Hash a one-time verification token for storage and comparison.
///
/// HMAC-SHA256 keyed with the server pepper (`TOKEN_PEPPER`); plain SHA-256 when no pepper is configured
/// (dev/test). Callers must compare hashes — raw tokens never touch the DB
/// or the logs.
pub fn verification_token_hash(token: &str) -> String {
  verification_token_hash_with_pepper(token, &crate::config::token_pepper())
}

/// Pure helper (explicit pepper) so tests stay deterministic without
/// mutating process-global env vars.
pub fn verification_token_hash_with_pepper(
  token: &str,
  pepper: &str,
) -> String {
  if pepper.is_empty() {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
  } else {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(pepper.as_bytes())
      .expect("HMAC accepts any key length");
    mac.update(token.as_bytes());
    hex::encode(mac.finalize().into_bytes())
  }
}

/// First 8 chars of a token hash — the only token-derived value allowed in
/// logs.
pub fn verification_token_hash_prefix(hash: &str) -> &str {
  hash.get(..8).unwrap_or(hash)
}

fn system_time_to_primitive(st: SystemTime) -> PrimitiveDateTime {
  let duration = st.duration_since(UNIX_EPOCH).unwrap_or_default();
  let odt =
    time::OffsetDateTime::from_unix_timestamp(duration.as_secs() as i64)
      .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

fn now_plus(secs: i64) -> PrimitiveDateTime {
  let odt = time::OffsetDateTime::now_utc() + time::Duration::seconds(secs);
  PrimitiveDateTime::new(odt.date(), odt.time())
}

/// Argon2id backup-code hashing/verification is blocking CPU work: run it
/// on the blocking pool with a 5s bound. `guard_pending_2fa` (attempt
/// cap) must already have run before any verify call so expensive hashes
/// cannot be used as a DoS amplifier. Hash failures are 500s
/// (server-side); verify timeouts fail closed as 401.
async fn hash_backup_codes_async(codes: Vec<String>) -> Result<String> {
  tokio::time::timeout(
    Duration::from_secs(5),
    tokio::task::spawn_blocking(move || hash_backup_codes(&codes)),
  )
  .await
  .map_err(|_| {
    crate::internal_error::internal_error(&"backup code hashing timed out")
  })?
  .map_err(|_| {
    crate::internal_error::internal_error(&"backup code task failed")
  })?
  .map_err(|e| crate::internal_error::internal_error(&e))
}

async fn consume_backup_code_async(
  stored: String,
  code: String,
) -> Result<String> {
  let remaining = tokio::time::timeout(
    Duration::from_secs(5),
    tokio::task::spawn_blocking(move || consume_backup_code(&stored, &code)),
  )
  .await
  .map_err(|_| unauthorized())?
  .map_err(|_| unauthorized())?;
  remaining.ok_or_else(unauthorized).map_err(Into::into)
}

pub fn is_admin(role: Option<&str>) -> bool {
  role.is_some_and(|r| r.eq_ignore_ascii_case("admin"))
}

pub async fn load_user(pool: &PgPool, user_id: Uuid) -> Result<User> {
  let row = sqlx::query!(
    r#"
        SELECT id, name, email, email_verified, image, two_factor_enabled, role,
               banned, ban_reason, ban_expires
        FROM users WHERE id = $1
        "#,
    user_id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| bad_request("user not found"))?;

  Ok(User {
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
  })
}

pub async fn start_session_for_user(
  cx: &Cx,
  pool: &PgPool,
  user: User,
  impersonated_by: Option<Uuid>,
) -> Result<SessionUser> {
  let session = session::start(cx).await?;
  let (ip, ua) = client_meta(cx);
  let session_id = session_store::insert_session(
    pool,
    &session,
    user.id,
    ip.as_deref(),
    ua.as_deref(),
    impersonated_by,
  )
  .await?;

  if impersonated_by.is_none() {
    crate::app::store::cart::merge_guest_cart_on_login(cx, pool, user.id)
      .await?;
  }

  Ok(SessionUser {
    user,
    session_id,
    expires_at: system_time_to_primitive(session.expires_at),
    impersonated_by,
  })
}

// --- Pending 2FA cookie (HTML login flow) ---

pub fn set_pending_2fa_cookie(cx: &Cx, token: &str) {
  let secure = pending_2fa_cookie_secure();
  let mut builder =
    Cookie::build((pending_2fa_cookie_name(), token.to_owned()))
      .path("/")
      .http_only(true)
      .same_site(SameSite::Lax)
      .max_age(topcoat::cookie::time::Duration::minutes(10));
  if secure {
    builder = builder.secure(true);
  }
  cookies(cx).add(builder.build());
}

pub fn clear_pending_2fa_cookie(cx: &Cx) {
  let secure = pending_2fa_cookie_secure();
  let mut builder = Cookie::build((pending_2fa_cookie_name(), "")).path("/");
  builder = builder.http_only(true).same_site(SameSite::Lax);
  if secure {
    builder = builder.secure(true);
  }
  cookies(cx).remove(builder.build());
}

pub fn read_pending_2fa_cookie(cx: &Cx) -> Option<String> {
  cookies(cx)
    .get(pending_2fa_cookie_name())
    .map(|c| c.value().to_owned())
    .filter(|t| !t.is_empty())
}

// --- Sign up / sign in ---

pub async fn sign_up_email(
  cx: &Cx,
  pool: &PgPool,
  name: &str,
  email: &str,
  password: &str,
) -> Result<SessionUser> {
  let name = crate::auth::validation::validate_name(name)
    .map_err(|err| bad_request(err.to_string()))?;
  let email = crate::auth::validation::validate_email(email)
    .map_err(|err| bad_request(err.to_string()))?;
  if let Err(err) =
    validate_password(password, &[email.as_str(), name.as_str(), "lindaflor"])
  {
    return Err(bad_request(err.to_string()).into());
  }

  if sqlx::query_scalar!(
    r#"SELECT id AS "id!" FROM users WHERE email = $1"#,
    email
  )
  .fetch_optional(pool)
  .await?
  .is_some()
  {
    return Err(bad_request("unable to complete sign-up").into());
  }

  let password_hash = hash_password(password)?;
  let user_id = Uuid::now_v7();
  let account_id = Uuid::now_v7();

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

  start_session_for_user(
    cx,
    pool,
    User {
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
    None,
  )
  .await
}

#[derive(Debug)]
pub enum SignInOutcome {
  SignedIn(SessionUser),
  TwoFactorRequired { token: String },
}

pub async fn sign_in_email(
  cx: &Cx,
  pool: &PgPool,
  email: &str,
  password: &str,
) -> Result<SignInOutcome> {
  let email = normalize_email(email);
  if email.is_empty() || password.is_empty() {
    return Err(bad_request("email and password are required").into());
  }

  let ip = crate::valkey::client_ip(cx);
  let conn = crate::valkey::conn_from(cx);
  if crate::valkey::is_auth_locked_out(conn.clone(), &ip, &email).await {
    return Err(crate::rate_limit::rate_limit_error());
  }

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
    crate::valkey::record_auth_fail(conn, &ip, &email).await;
    return Err(unauthorized().into());
  };

  let Some(stored_hash) = row.password.as_deref() else {
    crate::valkey::record_auth_fail(conn, &ip, &email).await;
    return Err(unauthorized().into());
  };

  if !verify_password(password, stored_hash)? {
    crate::valkey::record_auth_fail(conn, &ip, &email).await;
    return Err(unauthorized().into());
  }

  crate::valkey::clear_auth_fails(conn, &ip, &email).await;

  if is_currently_banned(row.banned, row.ban_expires) {
    return Err(unauthorized().into());
  }

  if row.two_factor_enabled {
    let token = create_pending_2fa(cx, pool, row.id).await?;
    return Ok(SignInOutcome::TwoFactorRequired { token });
  }

  let su = start_session_for_user(
    cx,
    pool,
    User {
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
    None,
  )
  .await?;

  Ok(SignInOutcome::SignedIn(su))
}

pub async fn start_google_oauth(
  pool: &PgPool,
  callback_url: Option<&str>,
  link_user_id: Option<Uuid>,
  session_binding: Option<&str>,
) -> Result<String> {
  let Some(config) = GoogleOAuthConfig::from_env() else {
    return Err(bad_request("Google sign-in is not configured").into());
  };

  if link_user_id.is_some()
    && session_binding
      .map(str::trim)
      .filter(|s| !s.is_empty())
      .is_none()
  {
    return Err(bad_request("OAuth link requires an active session").into());
  }

  let callback_path = safe_callback_path(callback_url);
  let state = random_token();
  let code_verifier = random_token();
  let nonce = random_token();
  let code_challenge = pkce_s256_challenge(&code_verifier);
  store_oauth_state_full(
    pool,
    NewOAuthState {
      state: &state,
      callback_path: &callback_path,
      ttl_secs: OAUTH_STATE_TTL_SECS,
      link_user_id,
      session_binding,
      code_verifier: Some(&code_verifier),
      nonce: Some(&nonce),
    },
  )
  .await?;

  Ok(google_authorize_url(
    &config,
    &state,
    Some(&code_challenge),
    Some(&nonce),
  ))
}

// --- Password reset ---

pub async fn request_password_reset(
  pool: &PgPool,
  email: &str,
  redirect_to: Option<&str>,
) -> Result<()> {
  let email = normalize_email(email);
  if email.is_empty() {
    return Ok(());
  }

  let user =
    sqlx::query!(r#"SELECT id, name FROM users WHERE email = $1"#, email)
      .fetch_optional(pool)
      .await?;

  if let Some(user) = user {
    let token = random_token();
    let token_hash = verification_token_hash(&token);
    let token_hash_prefix =
      verification_token_hash_prefix(&token_hash).to_owned();
    let id = Uuid::now_v7();
    let identifier = format!("{RESET_IDENTIFIER_PREFIX}{email}");
    let expires_at = now_plus(RESET_TOKEN_TTL_SECS);

    // Invalidate outstanding reset tokens so only the newest works.
    sqlx::query!(
      r#"DELETE FROM verifications WHERE identifier = $1"#,
      identifier,
    )
    .execute(pool)
    .await?;

    sqlx::query!(
            r#"
            INSERT INTO verifications (id, identifier, value, value_hash, expires_at, created_at, updated_at)
            VALUES ($1, $2, '', $3, $4, now(), now())
            "#,
            id,
            identifier,
            token_hash,
            expires_at,
        )
        .execute(pool)
        .await?;

    tracing::info!(
        user_id = %user.id,
        email = %email,
        redirect_to = ?redirect_to,
        token_hash_prefix = %token_hash_prefix,
        "password reset token created (mail not wired yet)"
    );
  }

  Ok(())
}

pub async fn reset_password(
  pool: &PgPool,
  token: &str,
  new_password: &str,
) -> Result<()> {
  if token.trim().is_empty() {
    return Err(bad_request("token is required").into());
  }

  let token_hash = verification_token_hash(token.trim());
  let verification = sqlx::query!(
    r#"
        SELECT id, identifier
        FROM verifications
        WHERE value_hash = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    token_hash,
    format!("{RESET_IDENTIFIER_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?;

  let Some(verification) = verification else {
    return Err(bad_request("invalid or expired token").into());
  };

  let email = verification
    .identifier
    .strip_prefix(RESET_IDENTIFIER_PREFIX)
    .unwrap_or(&verification.identifier);
  let email = normalize_email(email);

  let user = sqlx::query!(r#"SELECT id FROM users WHERE email = $1"#, email)
    .fetch_optional(pool)
    .await?;

  let Some(user) = user else {
    return Err(bad_request("invalid or expired token").into());
  };

  // Authoritative policy check (with the account email as zxcvbn input)
  // before hashing or touching credential rows.
  if let Err(err) =
    validate_password(new_password, &[email.as_str(), "lindaflor"])
  {
    return Err(bad_request(err.to_string()).into());
  }

  let password_hash = hash_password(new_password)?;

  let updated = sqlx::query!(
    r#"
        UPDATE accounts
        SET password = $1, updated_at = now()
        WHERE user_id = $2 AND provider_id = $3
        "#,
    password_hash,
    user.id,
    CREDENTIAL_PROVIDER_ID,
  )
  .execute(pool)
  .await?;

  if updated.rows_affected() == 0 {
    return Err(bad_request("no credential account for user").into());
  }

  // Single-use: consume every outstanding reset token for this email.
  sqlx::query!(
    "DELETE FROM verifications WHERE identifier = $1",
    verification.identifier
  )
  .execute(pool)
  .await?;

  session_store::delete_all_for_user(pool, user.id).await?;
  Ok(())
}

// --- Email verification ---

pub async fn send_verification_email(
  pool: &PgPool,
  email: &str,
  callback_url: Option<&str>,
) -> Result<()> {
  let email = normalize_email(email);
  if email.is_empty() {
    return Err(bad_request("email is required").into());
  }

  let user = sqlx::query!(
    r#"SELECT id, name, email_verified FROM users WHERE email = $1"#,
    email
  )
  .fetch_optional(pool)
  .await?;

  if let Some(user) = user
    && !user.email_verified
  {
    let token = random_token();
    let token_hash = verification_token_hash(&token);
    let token_hash_prefix =
      verification_token_hash_prefix(&token_hash).to_owned();
    let id = Uuid::now_v7();
    let identifier = format!("{VERIFY_IDENTIFIER_PREFIX}{email}");
    let expires_at = now_plus(VERIFY_TOKEN_TTL_SECS);

    // Only the newest verification token stays valid.
    sqlx::query!(
      r#"DELETE FROM verifications WHERE identifier = $1"#,
      identifier,
    )
    .execute(pool)
    .await?;

    sqlx::query!(
                r#"
                INSERT INTO verifications (id, identifier, value, value_hash, expires_at, created_at, updated_at)
                VALUES ($1, $2, '', $3, $4, now(), now())
                "#,
                id,
                identifier,
                token_hash,
                expires_at,
            )
            .execute(pool)
            .await?;

    tracing::info!(
        user_id = %user.id,
        email = %email,
        callback_url = ?callback_url,
        token_hash_prefix = %token_hash_prefix,
        "email verification token created (mail not wired yet)"
    );
  }

  Ok(())
}

pub async fn consume_email_verification_token(
  pool: &PgPool,
  token: &str,
) -> Result<()> {
  if token.trim().is_empty() {
    return Err(bad_request("token is required").into());
  }

  let token_hash = verification_token_hash(token.trim());
  let verification = sqlx::query!(
    r#"
        SELECT id, identifier
        FROM verifications
        WHERE value_hash = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    token_hash,
    format!("{VERIFY_IDENTIFIER_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?;

  let Some(verification) = verification else {
    return Err(bad_request("invalid or expired token").into());
  };

  let email = verification
    .identifier
    .strip_prefix(VERIFY_IDENTIFIER_PREFIX)
    .unwrap_or(&verification.identifier);
  let email = normalize_email(email);

  let updated = sqlx::query!(
    r#"
        UPDATE users
        SET email_verified = true, updated_at = now()
        WHERE email = $1
        "#,
    email,
  )
  .execute(pool)
  .await?;

  if updated.rows_affected() == 0 {
    return Err(unauthorized().into());
  }

  sqlx::query!("DELETE FROM verifications WHERE id = $1", verification.id)
    .execute(pool)
    .await?;

  Ok(())
}

// --- Two-factor (pending sign-in) ---

async fn resolve_pending_user_id(
  pool: &PgPool,
  token: &str,
) -> Result<(Uuid, Uuid)> {
  let token_hash = verification_token_hash(token.trim());
  let verification = sqlx::query!(
    r#"
        SELECT id, identifier
        FROM verifications
        WHERE value_hash = $1
          AND identifier LIKE $2
          AND expires_at > now()
        "#,
    token_hash,
    format!("{PENDING_2FA_PREFIX}%"),
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(unauthorized)?;

  let user_id_str = verification
    .identifier
    .strip_prefix(PENDING_2FA_PREFIX)
    .ok_or_else(unauthorized)?;
  let user_id = Uuid::parse_str(user_id_str).map_err(|_| unauthorized())?;
  Ok((verification.id, user_id))
}

pub async fn complete_2fa_with_totp(
  cx: &Cx,
  pool: &PgPool,
  pending_token: &str,
  code: &str,
) -> Result<SessionUser> {
  let code = code.trim();
  if code.is_empty() {
    return Err(bad_request("code is required").into());
  }

  let (verification_id, user_id) =
    resolve_pending_user_id(pool, pending_token.trim()).await?;
  let tf = sqlx::query!(
    r#"SELECT secret_nonce, secret_ciphertext, secret_key_version, verified FROM two_factor WHERE user_id = $1"#,
    user_id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(unauthorized)?;

  if !tf.verified {
    return Err(unauthorized().into());
  }
  guard_pending_2fa(cx, pool, pending_token.trim(), user_id, verification_id)
    .await?;
  // Fail closed on any decrypt error (wrong key or tampered row):
  // indistinguishable from a wrong code here.
  let secret = decrypt_row_secret(
    tf.secret_nonce,
    tf.secret_ciphertext,
    tf.secret_key_version,
  )
  .map_err(|_| unauthorized())?;
  if !verify_code(secret.as_str(), code).map_err(bad_request)? {
    return Err(unauthorized().into());
  }
  if !crate::valkey::totp_reserve(crate::valkey::conn_from(cx), &user_id, code)
    .await
  {
    return Err(unauthorized().into());
  }

  sqlx::query!("DELETE FROM verifications WHERE id = $1", verification_id)
    .execute(pool)
    .await?;
  crate::valkey::pending_clear(
    crate::valkey::conn_from(cx),
    pending_token.trim(),
  )
  .await;

  let user = load_user(pool, user_id).await?;
  if is_currently_banned(user.banned, user.ban_expires) {
    return Err(unauthorized().into());
  }
  start_session_for_user(cx, pool, user, None).await
}

pub async fn complete_2fa_with_backup(
  cx: &Cx,
  pool: &PgPool,
  pending_token: &str,
  code: &str,
) -> Result<SessionUser> {
  let code = code.trim();
  if code.is_empty() {
    return Err(bad_request("code is required").into());
  }

  let (verification_id, user_id) =
    resolve_pending_user_id(pool, pending_token.trim()).await?;
  let tf = sqlx::query!(
    r#"SELECT id, backup_codes, verified FROM two_factor WHERE user_id = $1"#,
    user_id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(unauthorized)?;

  if !tf.verified {
    return Err(unauthorized().into());
  }
  guard_pending_2fa(cx, pool, pending_token.trim(), user_id, verification_id)
    .await?;

  // Rate-limit guard above runs BEFORE the Argon2 work below.
  let stored =
    consume_backup_code_async(tf.backup_codes, code.to_owned()).await?;
  sqlx::query!(
    r#"UPDATE two_factor SET backup_codes = $2 WHERE id = $1"#,
    tf.id,
    stored,
  )
  .execute(pool)
  .await?;

  sqlx::query!("DELETE FROM verifications WHERE id = $1", verification_id)
    .execute(pool)
    .await?;
  crate::valkey::pending_clear(
    crate::valkey::conn_from(cx),
    pending_token.trim(),
  )
  .await;

  let user = load_user(pool, user_id).await?;
  if is_currently_banned(user.banned, user.ban_expires) {
    return Err(unauthorized().into());
  }
  start_session_for_user(cx, pool, user, None).await
}

// --- Profile / account ---

pub async fn update_user_name(
  pool: &PgPool,
  user_id: Uuid,
  name: &str,
) -> Result<User> {
  let name = crate::auth::validation::validate_name(name)
    .map_err(|err| bad_request(err.to_string()))?;

  sqlx::query!(
    r#"UPDATE users SET name = $2, updated_at = now() WHERE id = $1"#,
    user_id,
    name,
  )
  .execute(pool)
  .await?;

  load_user(pool, user_id).await
}

pub async fn change_password(
  pool: &PgPool,
  user_id: Uuid,
  session_id: Uuid,
  current_password: &str,
  new_password: &str,
  revoke_other_sessions: bool,
) -> Result<()> {
  if current_password.is_empty() {
    return Err(bad_request("currentPassword is required").into());
  }
  // Authoritative policy check (with the account name/email as zxcvbn
  // input) before verifying or re-hashing anything. `load_user` reuses the
  // cached user lookup; a missing row falls back to the site name alone
  // and surfaces as unauthorized below.
  let existing = load_user(pool, user_id).await.ok();
  let (account_name, account_email) = existing
    .map(|user| (user.name, user.email))
    .unwrap_or_default();
  if let Err(err) = validate_password(
    new_password,
    &[account_email.as_str(), account_name.as_str(), "lindaflor"],
  ) {
    return Err(bad_request(err.to_string()).into());
  }

  let account = sqlx::query!(
    r#"
        SELECT password
        FROM accounts
        WHERE user_id = $1 AND provider_id = $2
        "#,
    user_id,
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

  if !verify_password(current_password, stored_hash)? {
    return Err(unauthorized().into());
  }

  let password_hash = hash_password(new_password)?;
  sqlx::query!(
    r#"
        UPDATE accounts
        SET password = $1, updated_at = now()
        WHERE user_id = $2 AND provider_id = $3
        "#,
    password_hash,
    user_id,
    CREDENTIAL_PROVIDER_ID,
  )
  .execute(pool)
  .await?;

  if revoke_other_sessions {
    session_store::delete_all_for_user_except(pool, user_id, session_id)
      .await?;
  }

  Ok(())
}

pub async fn request_change_email(
  pool: &PgPool,
  user_id: Uuid,
  current_email: &str,
  new_email: &str,
  callback_url: Option<&str>,
) -> Result<()> {
  let new_email = crate::auth::validation::validate_email(new_email)
    .map_err(|err| bad_request(err.to_string()))?;
  if new_email == normalize_email(current_email) {
    return Err(bad_request("newEmail must differ from current email").into());
  }

  let taken = sqlx::query!(
    r#"SELECT id FROM users WHERE email = $1 AND id <> $2"#,
    new_email,
    user_id,
  )
  .fetch_optional(pool)
  .await?;

  if taken.is_some() {
    tracing::info!(user_id = %user_id, "change-email skipped; address in use");
    return Ok(());
  }

  let identifier_like = format!("{CHANGE_EMAIL_IDENTIFIER_PREFIX}{user_id}:%");
  sqlx::query!(
    r#"DELETE FROM verifications WHERE identifier LIKE $1"#,
    identifier_like,
  )
  .execute(pool)
  .await?;

  let token = random_token();
  let token_hash = verification_token_hash(&token);
  let token_hash_prefix =
    verification_token_hash_prefix(&token_hash).to_owned();
  let id = Uuid::now_v7();
  let identifier =
    format!("{CHANGE_EMAIL_IDENTIFIER_PREFIX}{user_id}:{new_email}");
  let expires_at = now_plus(CHANGE_EMAIL_TOKEN_TTL_SECS);

  sqlx::query!(
        r#"
        INSERT INTO verifications (id, identifier, value, value_hash, expires_at, created_at, updated_at)
        VALUES ($1, $2, '', $3, $4, now(), now())
        "#,
        id,
        identifier,
        token_hash,
        expires_at,
    )
    .execute(pool)
    .await?;

  tracing::info!(
      user_id = %user_id,
      current_email = %current_email,
      new_email = %new_email,
      callback_url = ?callback_url,
      token_hash_prefix = %token_hash_prefix,
      confirm_path = "/api/auth/change-email/confirm",
      "change-email confirmation token created (mail not wired yet)"
  );

  Ok(())
}

// --- Sessions ---

#[derive(Debug, Clone)]
pub struct ListedSession {
  pub id: Uuid,
  pub user_agent: Option<String>,
  pub ip_address: Option<String>,
  pub created_at: String,
  pub expires_at: String,
  pub current: bool,
}

pub async fn list_user_sessions(
  cx: &Cx,
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Vec<ListedSession>> {
  let current_token =
    session::token_hash(cx).await?.map(|h| token_hash_hex(&h));

  let rows = sqlx::query!(
    r#"
        SELECT id, token, user_agent, ip_address, created_at, expires_at
        FROM sessions
        WHERE user_id = $1
          AND expires_at > now()
        ORDER BY created_at DESC
        "#,
    user_id,
  )
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| {
        let current = current_token
          .as_deref()
          .is_some_and(|t| t == r.token.as_str());
        ListedSession {
          id: r.id,
          user_agent: r.user_agent,
          ip_address: r.ip_address,
          created_at: format_primitive(r.created_at),
          expires_at: format_primitive(r.expires_at),
          current,
        }
      })
      .collect(),
  )
}

pub async fn revoke_user_session(
  cx: &Cx,
  pool: &PgPool,
  user_id: Uuid,
  current_session_id: Uuid,
  session_id: Uuid,
) -> Result<()> {
  let deleted =
    session_store::delete_owned_session(pool, user_id, session_id).await?;
  if !deleted {
    return Err(bad_request("session not found").into());
  }
  if session_id == current_session_id {
    let _ = session::stop(cx).await?;
  }
  Ok(())
}

// --- Linked accounts ---

#[derive(Debug, Clone)]
pub struct LinkedAccount {
  pub id: Uuid,
  pub provider_id: String,
  pub account_id: String,
  pub created_at: String,
}

pub async fn list_linked_accounts(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Vec<LinkedAccount>> {
  let rows = sqlx::query!(
    r#"
        SELECT id, provider_id, account_id, created_at
        FROM accounts
        WHERE user_id = $1
        ORDER BY created_at ASC
        "#,
    user_id,
  )
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| LinkedAccount {
        id: r.id,
        provider_id: r.provider_id,
        account_id: r.account_id,
        created_at: format_primitive(r.created_at),
      })
      .collect(),
  )
}

pub async fn unlink_linked_account(
  pool: &PgPool,
  user_id: Uuid,
  provider_id: &str,
  account_id: &str,
) -> Result<()> {
  let provider_id = provider_id.trim();
  let account_id = account_id.trim();
  if provider_id.is_empty() || account_id.is_empty() {
    return Err(bad_request("providerId and accountId are required").into());
  }

  let target = sqlx::query!(
    r#"
        SELECT id, provider_id, password
        FROM accounts
        WHERE user_id = $1 AND provider_id = $2 AND account_id = $3
        "#,
    user_id,
    provider_id,
    account_id,
  )
  .fetch_optional(pool)
  .await?;

  let Some(target) = target else {
    return Err(bad_request("account not found").into());
  };

  let others = sqlx::query!(
    r#"
        SELECT provider_id, password
        FROM accounts
        WHERE user_id = $1 AND id <> $2
        "#,
    user_id,
    target.id,
  )
  .fetch_all(pool)
  .await?;

  let has_other_login = others.iter().any(|a| {
    if a.provider_id == CREDENTIAL_PROVIDER_ID {
      a.password.as_deref().is_some_and(|p| !p.is_empty())
    } else {
      true
    }
  });

  if !has_other_login {
    return Err(
      bad_request("cannot unlink the only remaining login method").into(),
    );
  }

  sqlx::query!("DELETE FROM accounts WHERE id = $1", target.id)
    .execute(pool)
    .await?;

  Ok(())
}

pub async fn start_link_google(
  pool: &PgPool,
  user_id: Uuid,
  callback_url: Option<&str>,
  session_binding: &str,
) -> Result<String> {
  start_google_oauth(pool, callback_url, Some(user_id), Some(session_binding))
    .await
}

// --- Delete user ---

/// HTML settings shortcut: confirm email matches, then delete immediately
/// (mail confirmation is not wired yet).
///
/// Email match is the only check here. Callers MUST step-up first via
/// `authorize_immediate_delete` (password or TOTP + same-origin + fresh
/// session) in `src/app/settings.rs`. There is no API route to this
/// function. Soft-delete / 24h grace is not implemented (would need a
/// schema change); the HTML path instead requires a session younger than
/// 30 minutes.
pub async fn delete_user_confirmed(
  cx: &Cx,
  pool: &PgPool,
  user: &User,
  confirm_email: &str,
) -> Result<()> {
  if normalize_email(confirm_email) != normalize_email(&user.email) {
    return Err(bad_request("email confirmation does not match").into());
  }

  sqlx::query!(r#"DELETE FROM users WHERE id = $1"#, user.id)
    .execute(pool)
    .await?;

  if let Some(hash) = session::stop(cx).await? {
    let _ = session_store::delete_by_token_hash(pool, &hash).await;
  }

  Ok(())
}

pub async fn request_delete_user(
  pool: &PgPool,
  user: &User,
  password: Option<&str>,
  callback_url: Option<&str>,
) -> Result<()> {
  // Password re-check is mandatory on the request path whenever the
  // account has a credential password. OAuth-only accounts (no stored hash)
  // fall back to the authenticated session; TOTP re-check for those lives
  // at the HTTP route layer (`delete_user.rs`), which also accepts a code.
  let account = sqlx::query!(
    r#"
          SELECT password
          FROM accounts
          WHERE user_id = $1 AND provider_id = $2
          "#,
    user.id,
    CREDENTIAL_PROVIDER_ID,
  )
  .fetch_optional(pool)
  .await?;

  if let Some(stored) = account.and_then(|a| a.password).as_deref() {
    let Some(password) = password.filter(|p| !p.is_empty()) else {
      return Err(unauthorized().into());
    };
    if !verify_password(password, stored)? {
      return Err(unauthorized().into());
    }
  }

  let identifier = format!("{DELETE_IDENTIFIER_PREFIX}{}", user.id);
  sqlx::query!(
    r#"DELETE FROM verifications WHERE identifier = $1"#,
    identifier,
  )
  .execute(pool)
  .await?;

  let token = random_token();
  let token_hash = verification_token_hash(&token);
  let token_hash_prefix =
    verification_token_hash_prefix(&token_hash).to_owned();
  let id = Uuid::now_v7();
  let expires_at = now_plus(DELETE_TOKEN_TTL_SECS);

  sqlx::query!(
        r#"
        INSERT INTO verifications (id, identifier, value, value_hash, expires_at, created_at, updated_at)
        VALUES ($1, $2, '', $3, $4, now(), now())
        "#,
        id,
        identifier,
        token_hash,
        expires_at,
    )
    .execute(pool)
    .await?;

  tracing::info!(
      user_id = %user.id,
      email = %user.email,
      callback_url = ?callback_url,
      token_hash_prefix = %token_hash_prefix,
      confirm_path = "/api/auth/delete-user",
      "delete-user verification token created (mail not wired yet)"
  );

  Ok(())
}

// --- 2FA enable / disable (authenticated) ---

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnableTwoFactorResult {
  pub totp_uri: String,
  pub secret: String,
  pub backup_codes: Vec<String>,
}

pub async fn enable_two_factor(
  pool: &PgPool,
  user: &User,
) -> Result<EnableTwoFactorResult> {
  if user.two_factor_enabled {
    return Err(
      bad_request("two-factor authentication is already enabled").into(),
    );
  }

  let (secret, totp_uri) = generate_secret(&user.email).map_err(bad_request)?;
  let backup_codes = generate_backup_codes();
  let backup_stored = hash_backup_codes_async(backup_codes.clone()).await?;
  // Encrypt at enrollment: only the one-time response below ever carries
  // the plaintext.
  let encrypted = encrypt_secret(&secret)
    .map_err(|e| crate::internal_error::internal_error(&e))?;
  let id = Uuid::now_v7();

  sqlx::query!(r#"DELETE FROM two_factor WHERE user_id = $1"#, user.id)
    .execute(pool)
    .await?;

  sqlx::query!(
    r#"
        INSERT INTO two_factor (id, backup_codes, user_id, verified, secret_nonce, secret_ciphertext, secret_key_version)
        VALUES ($1, $2, $3, false, $4, $5, $6)
        "#,
    id,
    backup_stored,
    user.id,
    encrypted.nonce,
    encrypted.ciphertext,
    encrypted.key_version,
  )
  .execute(pool)
  .await?;

  Ok(EnableTwoFactorResult {
    totp_uri,
    secret,
    backup_codes,
  })
}

pub async fn confirm_enable_two_factor(
  cx: &Cx,
  pool: &PgPool,
  user: &User,
  code: &str,
) -> Result<()> {
  if user.two_factor_enabled {
    return Err(
      bad_request("two-factor authentication is already enabled").into(),
    );
  }

  let code = code.trim();
  if code.is_empty() {
    return Err(bad_request("code is required").into());
  }

  let tf = sqlx::query!(
    r#"SELECT id, secret_nonce, secret_ciphertext, secret_key_version, verified FROM two_factor WHERE user_id = $1"#,
    user.id,
  )
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| bad_request("call enable first"))?;

  if tf.verified {
    return Err(
      bad_request("two-factor authentication is already verified").into(),
    );
  }
  let secret = decrypt_row_secret(
    tf.secret_nonce,
    tf.secret_ciphertext,
    tf.secret_key_version,
  )
  .map_err(|_| unauthorized())?;
  if !verify_code(secret.as_str(), code).map_err(bad_request)? {
    return Err(unauthorized().into());
  }
  if !crate::valkey::totp_reserve(crate::valkey::conn_from(cx), &user.id, code)
    .await
  {
    return Err(unauthorized().into());
  }

  sqlx::query!(
    r#"UPDATE two_factor SET verified = true WHERE id = $1"#,
    tf.id,
  )
  .execute(pool)
  .await?;

  sqlx::query!(
        r#"UPDATE users SET two_factor_enabled = true, updated_at = now() WHERE id = $1"#,
        user.id,
    )
    .execute(pool)
    .await?;

  Ok(())
}

/// Whether an unverified TOTP enrollment exists for this user.
///
/// Strict one-time display: the pending secret is NEVER re-read from the
/// DB (sole exposure is the enable response). This selects only the row
/// id, so the settings page can offer the confirm-code form without
/// touching secret material.
pub async fn has_unverified_two_factor(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<bool> {
  let tf = sqlx::query!(
    r#"SELECT id FROM two_factor WHERE user_id = $1 AND verified = false"#,
    user_id,
  )
  .fetch_optional(pool)
  .await?;

  Ok(tf.is_some())
}

pub async fn disable_two_factor(
  pool: &PgPool,
  user: &User,
  password: Option<&str>,
  code: Option<&str>,
) -> Result<()> {
  let password_ok = match password.filter(|p| !p.is_empty()) {
    Some(pw) => {
      let row = sqlx::query!(
                r#"SELECT password FROM accounts WHERE user_id = $1 AND provider_id = $2"#,
                user.id,
                CREDENTIAL_PROVIDER_ID,
            )
            .fetch_optional(pool)
            .await?;
      match row.and_then(|r| r.password) {
        Some(stored) => verify_password(pw, &stored)?,
        None => false,
      }
    }
    None => false,
  };

  let totp_ok = if password_ok {
    false
  } else if let Some(code) = code.map(str::trim).filter(|c| !c.is_empty()) {
    let tf = sqlx::query!(
      r#"SELECT secret_nonce, secret_ciphertext, secret_key_version, verified FROM two_factor WHERE user_id = $1"#,
      user.id,
    )
    .fetch_optional(pool)
    .await?;
    match tf {
      Some(tf) if tf.verified => {
        let secret = decrypt_row_secret(
          tf.secret_nonce,
          tf.secret_ciphertext,
          tf.secret_key_version,
        )
        .map_err(|_| unauthorized())?;
        verify_code(secret.as_str(), code).map_err(bad_request)?
      }
      _ => false,
    }
  } else {
    false
  };

  if !password_ok && !totp_ok {
    return Err(unauthorized().into());
  }

  sqlx::query!(r#"DELETE FROM two_factor WHERE user_id = $1"#, user.id)
    .execute(pool)
    .await?;

  sqlx::query!(
        r#"UPDATE users SET two_factor_enabled = false, updated_at = now() WHERE id = $1"#,
        user.id,
    )
    .execute(pool)
    .await?;

  Ok(())
}

// --- Impersonation ---

pub async fn stop_impersonating(
  cx: &Cx,
  pool: &PgPool,
  su: &SessionUser,
) -> Result<SessionUser> {
  let Some(admin_id) = su.impersonated_by else {
    return Err(bad_request("not currently impersonating").into());
  };

  let admin = load_user(pool, admin_id).await?;
  if is_currently_banned(admin.banned, admin.ban_expires) {
    return Err(unauthorized().into());
  }

  if let Some(hash) = session::stop(cx).await? {
    session_store::delete_by_token_hash(pool, &hash).await?;
  }

  start_session_for_user(cx, pool, admin, None).await
}

pub async fn impersonate_user(
  cx: &Cx,
  pool: &PgPool,
  admin: &SessionUser,
  target_user_id: Uuid,
) -> Result<SessionUser> {
  if !is_admin(admin.user.role.as_deref()) {
    return Err(forbidden().into());
  }
  if admin.impersonated_by.is_some() {
    return Err(bad_request("already impersonating a user").into());
  }
  if target_user_id == admin.user.id {
    return Err(bad_request("cannot impersonate yourself").into());
  }

  let target = load_user(pool, target_user_id).await?;
  if is_admin(target.role.as_deref()) {
    return Err(forbidden().into());
  }
  if is_currently_banned(target.banned, target.ban_expires) {
    return Err(bad_request("cannot impersonate a banned user").into());
  }

  start_session_for_user(cx, pool, target, Some(admin.user.id)).await
}

// --- Admin user management ---

const DEFAULT_BAN_REASON: &str = "Sem motivo";

pub const ADMIN_USER_PAGE_SIZE: i64 = 10;

pub struct AdminUserList {
  pub users: Vec<User>,
  pub total: i64,
}

pub struct AdminSessionRow {
  pub id: Uuid,
  pub created_at: String,
  pub expires_at: String,
  pub ip_address: Option<String>,
  pub user_agent: Option<String>,
}

fn ensure_admin(actor: &SessionUser) -> Result<()> {
  if is_admin(actor.user.role.as_deref()) {
    Ok(())
  } else {
    Err(forbidden().into())
  }
}

fn ensure_not_self(
  actor: &SessionUser,
  target: Uuid,
  what: &'static str,
) -> Result<()> {
  if target == actor.user.id {
    return Err(bad_request(what).into());
  }
  Ok(())
}

fn ensure_not_admin_target(target: &User) -> Result<()> {
  if is_admin(target.role.as_deref()) {
    return Err(forbidden().into());
  }
  Ok(())
}

async fn record_audit_event<'e, E>(
  executor: E,
  aggregate_type: &str,
  aggregate_id: &str,
  entity_type: &str,
  entity_id: Uuid,
  actor_id: Option<Uuid>,
  actor_name: Option<&str>,
  metadata: serde_json::Value,
) -> Result<()>
where
  E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
  crate::audit::record_audit_event(
    executor,
    aggregate_type,
    aggregate_id,
    entity_type,
    entity_id,
    actor_id,
    actor_name,
    metadata,
  )
  .await?;
  Ok(())
}

fn canonical_role(role: &str) -> Result<&'static str> {
  if role.trim().eq_ignore_ascii_case("admin") {
    Ok("admin")
  } else if role.trim().eq_ignore_ascii_case("moderator") {
    Ok("moderator")
  } else if role.trim().eq_ignore_ascii_case("user") {
    Ok("user")
  } else {
    Err(bad_request("invalid role").into())
  }
}

fn escape_like(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  for c in input.chars() {
    if matches!(c, '%' | '_' | '\\') {
      out.push('\\');
    }
    out.push(c);
  }
  out
}

fn like_pattern(query: Option<&str>) -> String {
  match query.map(str::trim).filter(|s| !s.is_empty()) {
    Some(q) => format!("%{}%", escape_like(q)),
    None => String::new(),
  }
}

fn clamp_limit_offset(limit: i64, offset: i64) -> (i64, i64) {
  (limit.clamp(1, 100), offset.max(0))
}

pub async fn list_admin_users(
  pool: &PgPool,
  actor: &SessionUser,
  query: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<AdminUserList> {
  ensure_admin(actor)?;
  let (limit, offset) = clamp_limit_offset(limit, offset);
  let pattern = like_pattern(query);

  let total: i64 = sqlx::query_scalar!(
    r#"
        SELECT COUNT(*) FROM users
        WHERE ($1 = '' OR name ILIKE $1 OR email ILIKE $1)
        "#,
    pattern,
  )
  .fetch_one(pool)
  .await?
  .unwrap_or(0);

  let rows = sqlx::query!(
    r#"
        SELECT id, name, email, email_verified, image, two_factor_enabled, role,
               banned, ban_reason, ban_expires
        FROM users
        WHERE ($1 = '' OR name ILIKE $1 OR email ILIKE $1)
        ORDER BY created_at DESC, id DESC
        LIMIT $2 OFFSET $3
        "#,
    pattern,
    limit,
    offset,
  )
  .fetch_all(pool)
  .await?;

  let users = rows
    .into_iter()
    .map(|r| User {
      id: r.id,
      name: r.name,
      email: r.email,
      email_verified: r.email_verified,
      image: r.image,
      two_factor_enabled: r.two_factor_enabled,
      role: r.role,
      banned: r.banned,
      ban_reason: r.ban_reason,
      ban_expires: r.ban_expires,
    })
    .collect();

  Ok(AdminUserList { users, total })
}

pub async fn set_user_role(
  pool: &PgPool,
  actor: &SessionUser,
  user_id: Uuid,
  role: &str,
) -> Result<User> {
  ensure_admin(actor)?;
  ensure_not_self(actor, user_id, "cannot change own role")?;
  let role = canonical_role(role)?;
  let target = load_user(pool, user_id).await?;
  ensure_not_admin_target(&target)?;
  let mut tx = pool.begin().await?;
  sqlx::query!(
    r#"UPDATE users SET role = $2, updated_at = now() WHERE id = $1"#,
    user_id,
    role,
  )
  .execute(&mut *tx)
  .await?;
  record_audit_event(
    &mut *tx,
    "user",
    &user_id.to_string(),
    "user",
    user_id,
    Some(actor.user.id),
    Some(actor.user.name.as_str()),
    serde_json::json!({ "action": "set_role", "role": role }),
  )
  .await?;
  tx.commit().await?;
  load_user(pool, user_id).await
}

pub async fn admin_update_user_name(
  pool: &PgPool,
  actor: &SessionUser,
  user_id: Uuid,
  name: &str,
) -> Result<User> {
  ensure_admin(actor)?;
  let _ = load_user(pool, user_id).await?;
  update_user_name(pool, user_id, name).await
}

pub async fn ban_user(
  pool: &PgPool,
  actor: &SessionUser,
  user_id: Uuid,
  ban_reason: Option<&str>,
) -> Result<User> {
  ensure_admin(actor)?;
  ensure_not_self(actor, user_id, "cannot ban yourself")?;
  let target = load_user(pool, user_id).await?;
  ensure_not_admin_target(&target)?;
  let reason = ban_reason
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .unwrap_or(DEFAULT_BAN_REASON);

  let mut tx = pool.begin().await?;
  sqlx::query!(
    r#"
        UPDATE users
        SET banned = true, ban_reason = $2, ban_expires = NULL, updated_at = now()
        WHERE id = $1
        "#,
    user_id,
    reason,
  )
  .execute(&mut *tx)
  .await?;
  sqlx::query!(r#"DELETE FROM sessions WHERE user_id = $1"#, user_id)
    .execute(&mut *tx)
    .await?;
  record_audit_event(
    &mut *tx,
    "user",
    &user_id.to_string(),
    "user",
    user_id,
    Some(actor.user.id),
    Some(actor.user.name.as_str()),
    serde_json::json!({ "action": "ban_user", "reason": reason }),
  )
  .await?;
  tx.commit().await?;

  load_user(pool, user_id).await
}

pub async fn unban_user(
  pool: &PgPool,
  actor: &SessionUser,
  user_id: Uuid,
) -> Result<User> {
  ensure_admin(actor)?;
  ensure_not_self(actor, user_id, "cannot unban yourself")?;
  let target = load_user(pool, user_id).await?;
  ensure_not_admin_target(&target)?;
  let mut tx = pool.begin().await?;
  sqlx::query!(
    r#"
        UPDATE users
        SET banned = false, ban_reason = NULL, ban_expires = NULL, updated_at = now()
        WHERE id = $1
        "#,
    user_id,
  )
  .execute(&mut *tx)
  .await?;
  record_audit_event(
    &mut *tx,
    "user",
    &user_id.to_string(),
    "user",
    user_id,
    Some(actor.user.id),
    Some(actor.user.name.as_str()),
    serde_json::json!({ "action": "unban_user" }),
  )
  .await?;
  tx.commit().await?;
  load_user(pool, user_id).await
}

pub async fn remove_user(
  pool: &PgPool,
  actor: &SessionUser,
  user_id: Uuid,
) -> Result<()> {
  ensure_admin(actor)?;
  ensure_not_self(actor, user_id, "cannot remove yourself")?;
  let target = load_user(pool, user_id).await?;
  ensure_not_admin_target(&target)?;
  let mut tx = pool.begin().await?;
  sqlx::query!(r#"DELETE FROM users WHERE id = $1"#, user_id)
    .execute(&mut *tx)
    .await?;
  // `entity_id` has no FK so the audit row keeps the removed id: insert
  // AFTER the delete inside the same tx.
  record_audit_event(
    &mut *tx,
    "user",
    &user_id.to_string(),
    "user",
    user_id,
    Some(actor.user.id),
    Some(actor.user.name.as_str()),
    serde_json::json!({ "action": "remove_user" }),
  )
  .await?;
  tx.commit().await?;
  Ok(())
}

pub async fn list_admin_user_sessions(
  pool: &PgPool,
  actor: &SessionUser,
  user_id: Uuid,
) -> Result<Vec<AdminSessionRow>> {
  ensure_admin(actor)?;
  let _ = load_user(pool, user_id).await?;
  let rows = sqlx::query!(
    r#"
        SELECT id, user_agent, ip_address, created_at, expires_at
        FROM sessions
        WHERE user_id = $1
          AND expires_at > now()
        ORDER BY created_at DESC
        "#,
    user_id,
  )
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| AdminSessionRow {
        id: r.id,
        created_at: format_primitive(r.created_at),
        expires_at: format_primitive(r.expires_at),
        ip_address: r.ip_address,
        user_agent: r.user_agent,
      })
      .collect(),
  )
}

pub async fn revoke_admin_session(
  pool: &PgPool,
  actor: &SessionUser,
  session_id: Uuid,
  expected_user_id: Uuid,
) -> Result<()> {
  ensure_admin(actor)?;
  let owner: Option<Uuid> = sqlx::query_scalar!(
    r#"SELECT user_id FROM sessions WHERE id = $1"#,
    session_id,
  )
  .fetch_optional(pool)
  .await?;
  let Some(owner) = owner else {
    return Err(bad_request("session not found").into());
  };
  if expected_user_id != owner {
    return Err(forbidden().into());
  }
  ensure_not_self(actor, owner, "cannot revoke own session")?;
  let target = load_user(pool, owner).await?;
  ensure_not_admin_target(&target)?;
  let mut tx = pool.begin().await?;
  let deleted = sqlx::query!(
    r#"DELETE FROM sessions WHERE id = $1 AND user_id = $2"#,
    session_id,
    owner,
  )
  .execute(&mut *tx)
  .await?;
  if deleted.rows_affected() == 0 {
    return Err(bad_request("session not found").into());
  }
  record_audit_event(
    &mut *tx,
    "user",
    &owner.to_string(),
    "session",
    session_id,
    Some(actor.user.id),
    Some(actor.user.name.as_str()),
    serde_json::json!({ "action": "revoke_session", "session_id": session_id }),
  )
  .await?;
  tx.commit().await?;
  Ok(())
}

pub async fn revoke_admin_user_sessions(
  pool: &PgPool,
  actor: &SessionUser,
  user_id: Uuid,
) -> Result<()> {
  ensure_admin(actor)?;
  ensure_not_self(actor, user_id, "cannot revoke own sessions")?;
  let target = load_user(pool, user_id).await?;
  ensure_not_admin_target(&target)?;
  let mut tx = pool.begin().await?;
  sqlx::query!(r#"DELETE FROM sessions WHERE user_id = $1"#, user_id)
    .execute(&mut *tx)
    .await?;
  record_audit_event(
    &mut *tx,
    "user",
    &user_id.to_string(),
    "user",
    user_id,
    Some(actor.user.id),
    Some(actor.user.name.as_str()),
    serde_json::json!({ "action": "revoke_all_sessions" }),
  )
  .await?;
  tx.commit().await?;
  Ok(())
}

/// Map auth errors to short Portuguese UI messages for HTML pages.
pub fn portuguese_error_message(err: &topcoat::Error) -> String {
  let msg = err.to_string().to_ascii_lowercase();
  if msg.contains("already exists") {
    "Já existe uma conta com este e-mail.".into()
  } else if msg.contains("at least") {
    format!("A senha deve ter pelo menos {MIN_PASSWORD_LEN} caracteres.")
  } else if msg.contains("at most") {
    format!("A senha deve ter no máximo {MAX_PASSWORD_LEN} caracteres.")
  } else if msg.contains("too weak") {
    "Escolha uma senha mais forte.".into()
  } else if msg.contains("invalid or expired") {
    "Link inválido ou expirado.".into()
  } else if msg.contains("does not match") {
    "O e-mail informado não confere.".into()
  } else if msg.contains("already in use") {
    "Este e-mail já está em uso.".into()
  } else if msg.contains("must differ") {
    "O novo e-mail deve ser diferente do atual.".into()
  } else if msg.contains("not configured") {
    "Login com Google não está configurado.".into()
  } else if msg.contains("too many") || msg.contains("429") {
    "Muitas tentativas. Aguarde um momento e tente novamente.".into()
  } else if msg.contains("unauthorized") || msg.contains("forbidden") {
    "Credenciais inválidas.".into()
  } else if msg.contains("name") && msg.contains("required") {
    "Informe o nome.".into()
  } else if msg.contains("email") && msg.contains("required") {
    "Informe um e-mail válido.".into()
  } else {
    "Não foi possível concluir a operação. Tente novamente.".into()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::password::hash_password;
  use crate::test_support::fresh_pool;
  use sqlx::Row;

  fn unique_email(prefix: &str) -> String {
    format!("{prefix}-{}@example.com", Uuid::now_v7().simple())
  }

  async fn insert_credential_user(
    pool: &PgPool,
    email: &str,
    password: &str,
  ) -> Uuid {
    let user_id = Uuid::now_v7();
    let account_id = Uuid::now_v7();
    let password_hash = hash_password(password).unwrap();
    sqlx::query!(
      r#"
        INSERT INTO users (id, name, email, email_verified, role, created_at, updated_at)
        VALUES ($1, $2, $3, false, 'user', now(), now())
        "#,
      user_id,
      "Hash Test",
      email,
    )
    .execute(pool)
    .await
    .unwrap();
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
    .await
    .unwrap();
    user_id
  }

  #[test]
  fn hash_prefix_is_eight_hex_and_not_raw() {
    let raw = "super-secret-reset-token";
    let hash = verification_token_hash(raw);
    let prefix = verification_token_hash_prefix(&hash);
    assert_eq!(prefix.len(), 8);
    assert!(prefix.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(hash, raw);
    assert!(!hash.contains(raw));
    let log_line = format!("token_hash_prefix={prefix}");
    assert!(
      !log_line.contains(raw),
      "log line must not contain the raw secret"
    );
  }

  #[test]
  fn peppered_hash_differs_from_plain_sha256() {
    let raw = "token-with-pepper";
    let plain = verification_token_hash_with_pepper(raw, "");
    let hmac = verification_token_hash_with_pepper(raw, "server-pepper");
    assert_ne!(plain, hmac);
    assert_ne!(plain, raw);
    assert_eq!(plain.len(), 64);
    assert_eq!(hmac.len(), 64);
  }

  #[tokio::test]
  async fn hashed_reset_lookup_stores_empty_value() {
    let pool = fresh_pool().await;
    let email = unique_email("hash-reset");
    insert_credential_user(&pool, &email, "password123").await;

    request_password_reset(&pool, &email, None).await.unwrap();

    let identifier = format!("{RESET_IDENTIFIER_PREFIX}{email}");
    let row = sqlx::query(
      "SELECT value, value_hash FROM verifications WHERE identifier = $1",
    )
    .bind(&identifier)
    .fetch_one(&pool)
    .await
    .unwrap();
    let value: String = row.get("value");
    let value_hash: Option<String> = row.get("value_hash");
    assert!(value.is_empty(), "value column must be empty at rest");
    let stored_hash = value_hash.expect("value_hash");
    assert_eq!(stored_hash.len(), 64);

    let raw = "known-raw-reset-secret";
    let hash = verification_token_hash(raw);
    sqlx::query(
      "UPDATE verifications SET value_hash = $1 WHERE identifier = $2",
    )
    .bind(&hash)
    .bind(&identifier)
    .execute(&pool)
    .await
    .unwrap();

    reset_password(&pool, raw, "Harbor-Violet-Tiger-4821-Lantern!")
      .await
      .unwrap();

    let leftover: i64 = sqlx::query_scalar(
      "SELECT count(*) FROM verifications WHERE identifier = $1",
    )
    .bind(&identifier)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(leftover, 0);
  }

  #[tokio::test]
  async fn expired_reset_token_is_rejected() {
    let pool = fresh_pool().await;
    let email = unique_email("hash-expired");
    insert_credential_user(&pool, &email, "password123").await;

    let raw = "expired-raw-reset-secret";
    let hash = verification_token_hash(raw);
    let identifier = format!("{RESET_IDENTIFIER_PREFIX}{email}");
    sqlx::query!(
      r#"
            INSERT INTO verifications (id, identifier, value, value_hash, expires_at, created_at, updated_at)
            VALUES ($1, $2, '', $3, $4, now(), now())
            "#,
      Uuid::now_v7(),
      identifier,
      hash,
      now_plus(-60),
    )
    .execute(&pool)
    .await
    .unwrap();

    let err = reset_password(&pool, raw, "Harbor-Violet-Tiger-4821-Lantern!")
      .await
      .expect_err("expired token");
    let msg = err.to_string().to_ascii_lowercase();
    assert!(
      msg.contains("invalid or expired") || msg.contains("bad request"),
      "unexpected error: {msg}"
    );
  }

  #[tokio::test]
  async fn request_reset_deletes_prior_tokens() {
    let pool = fresh_pool().await;
    let email = unique_email("hash-dedupe");
    insert_credential_user(&pool, &email, "password123").await;
    let identifier = format!("{RESET_IDENTIFIER_PREFIX}{email}");

    request_password_reset(&pool, &email, None).await.unwrap();
    request_password_reset(&pool, &email, None).await.unwrap();

    let count: i64 = sqlx::query_scalar(
      "SELECT count(*) FROM verifications WHERE identifier = $1",
    )
    .bind(&identifier)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
  }
}
