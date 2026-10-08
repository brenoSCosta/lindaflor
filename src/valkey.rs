use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use redis::aio::MultiplexedConnection;
use sha2::{Digest, Sha256};

// Rate limiting, brute-force backoff,
// TOTP single-use and pending-2FA binding helpers.
//
// Design notes:
// - Coarse per-IP rate limits run in-process via `governor` (see
//   [`crate::rate_limit::RateLimitLayer`], registered in `src/app.rs`).
//   They are a DoS guard, not a security boundary.
// - Security-critical counters (auth failures, 2FA attempts, TOTP reuse,
//   pending-token bindings) live in Valkey so they survive restarts and are
//   shared across replicas. When no Valkey connection is registered on the
//   request context (e.g. API-only test routers) or Valkey errors, the same
//   checks fall back to process-local maps — fail-closed per process instead
//   of fail-open.
// - The binary (`src/main.rs`) uses `lindaflor::valkey`; there is no
//   `mod valkey` in the binary, so this file may use `crate::` paths.

pub async fn create_client(
  valkey_url: &str,
) -> Result<MultiplexedConnection, redis::RedisError> {
  let client = redis::Client::open(valkey_url)?;
  let conn = client.get_multiplexed_async_connection().await?;
  Ok(conn)
}

/// Clone the shared Valkey connection for this request, if one is
/// registered. API-only test routers register no Valkey context.
pub fn conn_from(cx: &topcoat::context::Cx) -> Option<MultiplexedConnection> {
  topcoat::context::try_app_context::<MultiplexedConnection>(cx).cloned()
}

// ---------------------------------------------------------------------------
// Client identity (mirrors `client_meta`, without depending on auth routes)
// ---------------------------------------------------------------------------

/// Best-effort client IP: first `X-Forwarded-For` entry, else `X-Real-IP`,
/// else `"unknown"`. Same source as session logging
/// (`auth::routes::dto::client_meta`).
pub fn client_ip(cx: &topcoat::context::Cx) -> String {
  let headers = topcoat::router::request::headers(cx);
  headers
    .get("x-forwarded-for")
    .and_then(|v| v.to_str().ok())
    .and_then(|v| v.split(',').next())
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(str::to_owned)
    .or_else(|| {
      headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
    })
    .unwrap_or_else(|| "unknown".to_string())
}

/// Best-effort User-Agent (empty string when absent).
pub fn client_ua(cx: &topcoat::context::Cx) -> String {
  topcoat::router::request::headers(cx)
    .get("user-agent")
    .and_then(|v| v.to_str().ok())
    .map(str::to_owned)
    .unwrap_or_default()
}

/// `hex(SHA-256(ip | 0x00 | ua))`, truncated to 32 chars. Stored instead of
/// raw IPs/UAs in binding records.
pub fn ip_ua_hash(ip: &str, ua: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(ip.as_bytes());
  hasher.update([0u8]);
  hasher.update(ua.as_bytes());
  hex::encode(hasher.finalize())[..32].to_owned()
}

/// Hash an opaque pending-2FA token for use in Valkey key names, so Valkey
/// keyspace / `MONITOR` output never carries a usable secret.
pub fn pending_token_hash(token: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(token.trim().as_bytes());
  hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Tiny Valkey wrappers (fail over to process-local maps)
// ---------------------------------------------------------------------------

async fn valkey_incr(
  conn: Option<MultiplexedConnection>,
  key: &str,
) -> Option<u64> {
  let mut conn = conn?;
  let count: u64 = redis::cmd("INCR")
    .arg(key)
    .query_async(&mut conn)
    .await
    .ok()?;
  Some(count)
}

async fn valkey_expire(
  conn: Option<MultiplexedConnection>,
  key: &str,
  ttl_secs: u64,
) {
  if let Some(mut conn) = conn {
    let _: redis::RedisResult<()> = redis::cmd("EXPIRE")
      .arg(key)
      .arg(ttl_secs)
      .query_async(&mut conn)
      .await;
  }
}

async fn valkey_get(
  conn: Option<MultiplexedConnection>,
  key: &str,
) -> Option<String> {
  let mut conn = conn?;
  redis::cmd("GET")
    .arg(key)
    .query_async(&mut conn)
    .await
    .ok()
    .flatten()
}

async fn valkey_del(conn: Option<MultiplexedConnection>, key: &str) {
  if let Some(mut conn) = conn {
    let _: redis::RedisResult<()> =
      redis::cmd("DEL").arg(key).query_async(&mut conn).await;
  }
}

/// Atomic `SET key val NX EX ttl`.
/// `Some(true)` = claimed, `Some(false)` = already held, `None` = no
/// connection or Redis error (caller should use the process-local fallback).
async fn valkey_set_nx_ex(
  conn: Option<MultiplexedConnection>,
  key: &str,
  val: &str,
  ttl_secs: u64,
) -> Option<bool> {
  let mut conn = conn?;
  let res: redis::RedisResult<Option<String>> = redis::cmd("SET")
    .arg(key)
    .arg(val)
    .arg("NX")
    .arg("EX")
    .arg(ttl_secs)
    .query_async(&mut conn)
    .await;
  match res {
    Ok(Some(_)) => Some(true),
    Ok(None) => Some(false),
    Err(_) => None,
  }
}

async fn valkey_set_ex(
  conn: Option<MultiplexedConnection>,
  key: &str,
  val: &str,
  ttl_secs: u64,
) {
  if let Some(mut conn) = conn {
    let _: redis::RedisResult<()> = redis::cmd("SET")
      .arg(key)
      .arg(val)
      .arg("EX")
      .arg(ttl_secs)
      .query_async(&mut conn)
      .await;
  }
}

// Process-local fallback: key -> (count/value, deadline).
type FallbackMap = HashMap<String, (String, Instant)>;

static FALLBACK: OnceLock<Mutex<FallbackMap>> = OnceLock::new();

fn fallback() -> &'static Mutex<FallbackMap> {
  FALLBACK.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fallback_incr(key: &str, ttl: Duration) -> u64 {
  let mut guard = fallback().lock().unwrap_or_else(|e| e.into_inner());
  // Opportunistic prune.
  if guard.len() > 4096 {
    let now = Instant::now();
    guard.retain(|_, (_, deadline)| *deadline > now);
  }
  let now = Instant::now();
  let count = match guard.get(key) {
    Some((v, deadline)) if *deadline > now => v.parse::<u64>().unwrap_or(0) + 1,
    _ => 1,
  };
  guard.insert(key.to_owned(), (count.to_string(), now + ttl));
  count
}

fn fallback_get(key: &str) -> Option<String> {
  let mut guard = fallback().lock().unwrap_or_else(|e| e.into_inner());
  match guard.get(key) {
    Some((v, deadline)) if *deadline > Instant::now() => Some(v.clone()),
    _ => {
      guard.remove(key);
      None
    }
  }
}

fn fallback_set_nx(key: &str, val: &str, ttl: Duration) -> bool {
  let mut guard = fallback().lock().unwrap_or_else(|e| e.into_inner());
  let now = Instant::now();
  if let Some((_, deadline)) = guard.get(key)
    && *deadline > now
  {
    return false;
  }
  guard.insert(key.to_owned(), (val.to_owned(), now + ttl));
  true
}

fn fallback_del(key: &str) {
  if let Ok(mut guard) = fallback().lock() {
    guard.remove(key);
  }
}

/// Failures tracked per `(ip, email)` for 15 minutes (escalating TTL, see
/// [`record_auth_fail`]). At [`AUTH_FAIL_MAX`] the pair is locked out.
pub const AUTH_FAIL_MAX: u64 = 10;
pub const AUTH_FAIL_WINDOW_SECS: u64 = 15 * 60;

pub fn auth_fail_key(ip: &str, email: &str) -> String {
  format!(
    "auth:fail:{}:{}",
    ip.trim(),
    email.trim().to_ascii_lowercase()
  )
}

/// Record a failed password attempt. The observation window escalates
/// exponentially (`60s << count`, capped at 15 min): persistent attackers
/// stay tracked longer, while a couple of typos followed by a success
/// (which clears the counter) are unaffected.
pub async fn record_auth_fail(
  conn: Option<MultiplexedConnection>,
  ip: &str,
  email: &str,
) -> u64 {
  let key = auth_fail_key(ip, email);
  let ttl_secs = 60u64
    .checked_shl(3)
    .unwrap_or(AUTH_FAIL_WINDOW_SECS)
    .min(AUTH_FAIL_WINDOW_SECS);
  if let Some(count) = valkey_incr(conn.clone(), &key).await {
    let shift = count.min(4) as u32;
    let ttl = 60u64.checked_shl(shift).unwrap_or(AUTH_FAIL_WINDOW_SECS);
    valkey_expire(conn, &key, ttl.min(AUTH_FAIL_WINDOW_SECS)).await;
    return count;
  }
  let _ = ttl_secs;
  fallback_incr(&key, Duration::from_secs(AUTH_FAIL_WINDOW_SECS))
}

pub async fn auth_fail_count(
  conn: Option<MultiplexedConnection>,
  ip: &str,
  email: &str,
) -> u64 {
  let key = auth_fail_key(ip, email);
  if let Some(v) = valkey_get(conn, &key).await {
    return v.parse::<u64>().unwrap_or(0);
  }
  fallback_get(&key)
    .and_then(|v| v.parse::<u64>().ok())
    .unwrap_or(0)
}

pub async fn is_auth_locked_out(
  conn: Option<MultiplexedConnection>,
  ip: &str,
  email: &str,
) -> bool {
  auth_fail_count(conn, ip, email).await >= AUTH_FAIL_MAX
}

pub async fn clear_auth_fails(
  conn: Option<MultiplexedConnection>,
  ip: &str,
  email: &str,
) {
  let key = auth_fail_key(ip, email);
  valkey_del(conn, &key).await;
  fallback_del(&key);
}

/// Valkey key claiming one TOTP code for one user. TTL 90s covers the
/// `skew=1` acceptance window (prev/current/next 30s step).
pub fn totp_used_key(user_id: &uuid::Uuid, code: &str) -> String {
  crate::auth::totp::totp_reuse_key(user_id, code)
}

/// Atomically claim `(user, code)`. Returns `false` when the code was
/// already used within the window → caller must reject as replay.
pub async fn totp_reserve(
  conn: Option<MultiplexedConnection>,
  user_id: &uuid::Uuid,
  code: &str,
) -> bool {
  let key = totp_used_key(user_id, code);
  match valkey_set_nx_ex(conn, &key, "1", 90).await {
    Some(claimed) => claimed,
    None => fallback_set_nx(&key, "1", Duration::from_secs(90)),
  }
}

/// Failed verification attempts allowed per pending token before the token
/// row is invalidated server-side.
pub const PENDING_MAX_ATTEMPTS: u64 = 5;
pub const PENDING_TTL_SECS: u64 = 10 * 60;

fn pending_attempts_key(token_hash: &str) -> String {
  format!("2fa:attempt:{token_hash}")
}

fn pending_binding_key(token_hash: &str) -> String {
  format!("2fa:bind:{token_hash}")
}

/// Increment (and return) the attempt counter for a pending token.
pub async fn pending_attempt(
  conn: Option<MultiplexedConnection>,
  token: &str,
) -> u64 {
  let key = pending_attempts_key(&pending_token_hash(token));
  if let Some(count) = valkey_incr(conn.clone(), &key).await {
    if count == 1 {
      valkey_expire(conn, &key, PENDING_TTL_SECS).await;
    }
    return count;
  }
  fallback_incr(&key, Duration::from_secs(PENDING_TTL_SECS))
}

pub async fn pending_attempts(
  conn: Option<MultiplexedConnection>,
  token: &str,
) -> u64 {
  let key = pending_attempts_key(&pending_token_hash(token));
  if let Some(v) = valkey_get(conn, &key).await {
    return v.parse::<u64>().unwrap_or(0);
  }
  fallback_get(&key)
    .and_then(|v| v.parse::<u64>().ok())
    .unwrap_or(0)
}

/// Drop attempt + binding state after a terminal outcome (success or
/// invalidation). The DB row itself is deleted by the caller (one-time).
pub async fn pending_clear(conn: Option<MultiplexedConnection>, token: &str) {
  let hash = pending_token_hash(token);
  valkey_del(conn.clone(), &pending_attempts_key(&hash)).await;
  valkey_del(conn, &pending_binding_key(&hash)).await;
  fallback_del(&pending_attempts_key(&hash));
  fallback_del(&pending_binding_key(&hash));
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingBindingOutcome {
  /// Request IP/UA matches the stored binding.
  Matched,
  /// No binding stored yet (e.g. token minted by a path that cannot pass
  /// `cx`); bound to this request now. First legitimate verifier wins the
  /// race; stolen tokens verified later from elsewhere mismatch.
  NewlyBound,
  /// Binding exists and differs → reject.
  Mismatch,
}

/// Bind a freshly minted pending token to `(user, ip, ua)` (eager path,
/// e.g. JSON sign-in which has `cx` at mint time).
pub async fn pending_bind_eager(
  conn: Option<MultiplexedConnection>,
  token: &str,
  user_id: &uuid::Uuid,
  ip: &str,
  ua: &str,
) {
  let key = pending_binding_key(&pending_token_hash(token));
  let val = format!("{}|{}", user_id.as_simple(), ip_ua_hash(ip, ua));
  valkey_set_ex(conn, &key, &val, PENDING_TTL_SECS).await;
  fallback_del(&key);
  let _ = fallback_set_nx(&key, &val, Duration::from_secs(PENDING_TTL_SECS));
}

/// Verify the request against the stored binding (or bind-on-first-seen).
pub async fn pending_check_binding(
  conn: Option<MultiplexedConnection>,
  token: &str,
  user_id: &uuid::Uuid,
  ip: &str,
  ua: &str,
) -> PendingBindingOutcome {
  let key = pending_binding_key(&pending_token_hash(token));
  let want = format!("{}|{}", user_id.as_simple(), ip_ua_hash(ip, ua));
  let stored = valkey_get(conn.clone(), &key)
    .await
    .or_else(|| fallback_get(&key));
  match stored {
    None => {
      // Bind-on-first-seen: claim so a concurrent verifier loses.
      let claimed =
        match valkey_set_nx_ex(conn, &key, &want, PENDING_TTL_SECS).await {
          Some(v) => v,
          None => {
            fallback_set_nx(&key, &want, Duration::from_secs(PENDING_TTL_SECS))
          }
        };
      if claimed {
        PendingBindingOutcome::NewlyBound
      } else {
        // Lost the race: re-read and compare.
        let stored2 = fallback_get(&key);
        if stored2.as_deref() == Some(want.as_str()) {
          PendingBindingOutcome::Matched
        } else {
          PendingBindingOutcome::Mismatch
        }
      }
    }
    Some(got) if got == want => PendingBindingOutcome::Matched,
    Some(_) => PendingBindingOutcome::Mismatch,
  }
}

// ---------------------------------------------------------------------------
// CAPTCHA gate (stub): env-gated, verification is a documented follow-up
// ---------------------------------------------------------------------------

/// Turnstile/hCaptcha gate. Reads `TURNSTILE_SITE_KEY`, `TURNSTILE_SECRET_KEY` and
/// `TURNSTILE_ENFORCE` (`"1"`/`"true"` to require a token).
///
/// Current status: **config gate only**. `verify_captcha_token` fail-opens
/// with a warning until real server-side verification lands (follow-up:
/// `POST https://challenges.cloudflare.com/turnstile/v0/siteverify`
/// with a shared `reqwest` client + 5s timeout, then flip to fail-closed
/// and wire `captcha_token` fields into sign-up / password-reset / guest
/// checkout DTOs).
pub struct CaptchaConfig {
  pub site_key: Option<String>,
  pub secret_configured: bool,
  pub enforced: bool,
}

pub fn captcha_config() -> CaptchaConfig {
  let non_empty =
    |key: &str| std::env::var(key).ok().filter(|v| !v.trim().is_empty());
  let enforced = std::env::var("TURNSTILE_ENFORCE")
    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    .unwrap_or(false);
  CaptchaConfig {
    site_key: non_empty("TURNSTILE_SITE_KEY"),
    secret_configured: non_empty("TURNSTILE_SECRET_KEY").is_some(),
    enforced,
  }
}

/// Whether a CAPTCHA token must be presented on abuse-prone anonymous
/// endpoints (sign-up, password-reset, guest checkout). False until an
/// operator sets `TURNSTILE_SECRET_KEY` + `TURNSTILE_ENFORCE=1`.
pub fn captcha_required() -> bool {
  let cfg = captcha_config();
  cfg.enforced && cfg.secret_configured
}

/// Stub verifier. Fail-open (logs once) until the siteverify integration
/// lands; see [`CaptchaConfig`] docs.
pub async fn verify_captcha_token(token: Option<&str>) -> bool {
  if !captcha_required() {
    return true;
  }
  if token.is_some_and(|t| !t.trim().is_empty()) {
    tracing::warn!(
      "TURNSTILE_ENFORCE=1 but server-side siteverify is not implemented; \
       accepting token presence only (follow-up before relying on this)"
    );
    return true;
  }
  false
}

#[cfg(test)]
mod tests {
  use super::*;
  use uuid::Uuid;

  #[tokio::test]
  async fn totp_reserve_second_claim_fails() {
    let user_id = Uuid::now_v7();
    assert!(totp_reserve(None, &user_id, "123456").await);
    assert!(!totp_reserve(None, &user_id, "123456").await);
    // A different code for the same user is still claimable.
    assert!(totp_reserve(None, &user_id, "654321").await);
  }

  #[tokio::test]
  async fn pending_check_binding_mismatch() {
    let user_id = Uuid::now_v7();
    let token = "unit-pending-bind-token";
    pending_bind_eager(None, token, &user_id, "1.1.1.1", "ua-a").await;
    assert_eq!(
      pending_check_binding(None, token, &user_id, "2.2.2.2", "ua-a").await,
      PendingBindingOutcome::Mismatch
    );
    assert_eq!(
      pending_check_binding(None, token, &user_id, "1.1.1.1", "ua-a").await,
      PendingBindingOutcome::Matched
    );
  }
}
