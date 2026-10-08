//! Shared integration-test helpers (HTTP + auth + embedded Postgres).
//!
//! Compiled once as a library, so helpers used by only some test binaries
//! do not trigger per-binary `dead_code` warnings.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::OnceCell;
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::router::{Body, OriginPolicy, Router, RouterBuilderDiscoverExt};
use topcoat::session::RouterBuilderSessionExt;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "session";

mod embedded_postgres;

pub struct TestEnv {
  pub database_url: String,
  /// Kept alive for the process lifetime.
  _postgres: embedded_postgres::RunningPostgres,
}

static ENV: OnceCell<Arc<TestEnv>> = OnceCell::const_new();

/// Shared env (one embedded PG). Each test opens its own pool against the migrated DB.
pub async fn env() -> Arc<TestEnv> {
  ENV
    .get_or_init(|| async {
      // Must be set before session_config() / router build.
      unsafe { std::env::set_var("APP_ENV", "development") };
      install_test_rate_limit_env();

      // Force inventory discover to see library routes.
      let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
      lindaflor::auth::routes::link_for_discover();
      let _ = lindaflor::api::health;
      let _ = lindaflor::logging::metrics_route::metrics;
      let _ = lindaflor::app::dashboard::page;
      let _ = lindaflor::app::login::page;
      let _ = lindaflor::app::app_sidebar::logout_page;

      let postgres = embedded_postgres::start().await;
      let database_name = format!("lindaflor_test_{}", Uuid::now_v7().simple());
      postgres
        .server()
        .create_database(&database_name)
        .await
        .expect("create database");
      let database_url = postgres.server().settings().url(&database_name);

      // Migrate once up front.
      let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("connect pool");
      // Relative to this crate's manifest dir.
      sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("migrate");
      pool.close().await;

      Arc::new(TestEnv {
        database_url,
        _postgres: postgres,
      })
    })
    .await
    .clone()
}

/// Per-test pool + router (isolates connection checkout across tests).
pub async fn pool_and_router() -> (PgPool, Router) {
  let env = env().await;
  let pool = PgPoolOptions::new()
    .max_connections(5)
    .acquire_timeout(Duration::from_secs(10))
    .connect(&env.database_url)
    .await
    .expect("connect pool");
  let router = build_auth_router(pool.clone());
  (pool, router)
}

/// Generous per-IP budgets so existing tests that hammer auth do not 429.
/// `limit_for` maps `auth:sign-in` → `RATE_LIMIT_AUTH_SIGN_IN_PER_MIN`.
fn install_test_rate_limit_env() {
  unsafe {
    std::env::set_var("RATE_LIMIT_GLOBAL_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_AUTH_SIGN_IN_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_AUTH_SIGN_UP_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_AUTH_PASSWORD_RESET_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_AUTH_VERIFY_EMAIL_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_AUTH_VERIFY_TOTP_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_AUTH_VERIFY_BACKUP_CODE_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_PEDIDO_POLL_PER_MIN", "10000");
    std::env::set_var("RATE_LIMIT_PEDIDO_PAGE_PER_MIN", "10000");
  }
}

pub fn build_auth_router(pool: PgPool) -> Router {
  unsafe { std::env::set_var("APP_ENV", "development") };
  install_test_rate_limit_env();
  let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
  lindaflor::auth::routes::link_for_discover();
  let _ = lindaflor::api::health;
  let _ = lindaflor::logging::metrics_route::metrics;
  let _ = lindaflor::app::dashboard::page;
  let _ = lindaflor::app::login::page;
  let _ = lindaflor::app::app_sidebar::logout_page;

  use lindaflor::rate_limit::{
    DEFAULT_BACKUP_CODE_PER_MIN, DEFAULT_GLOBAL_PER_MIN,
    DEFAULT_PASSWORD_RESET_PER_MIN, DEFAULT_PEDIDO_PAGE_PER_MIN,
    DEFAULT_PEDIDO_POLL_PER_MIN, DEFAULT_SIGN_IN_PER_MIN,
    DEFAULT_SIGN_UP_PER_MIN, DEFAULT_TOTP_PER_MIN,
    DEFAULT_VERIFY_EMAIL_PER_MIN, RateLimitLayer, SCOPE_BACKUP_CODE,
    SCOPE_PASSWORD_RESET, SCOPE_PEDIDO_PAGE, SCOPE_PEDIDO_POLL, SCOPE_SIGN_IN,
    SCOPE_SIGN_UP, SCOPE_TOTP, SCOPE_VERIFY_EMAIL,
  };

  let mut origin_policy = OriginPolicy::new();
  let mut builder = Router::builder()
    .discover()
    .cookies()
    .sessions(lindaflor::auth::session_config())
    .app_context(pool);
  if let Some(origin) = lindaflor::auth::google::app_origin() {
    builder = builder.base_url(origin.as_str());
    origin_policy = origin_policy.trust_origins([origin]);
  }
  builder = builder.origin_policy(origin_policy);
  // Mirror src/app.rs: fallback outermost among app layers, tower last.
  builder =
    lindaflor::security_headers::apply_security_headers_fallback(builder);
  builder = builder
    .layer(RateLimitLayer::global(DEFAULT_GLOBAL_PER_MIN))
    .layer(RateLimitLayer::scoped(
      "/api/auth/sign-in",
      SCOPE_SIGN_IN,
      DEFAULT_SIGN_IN_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/sign-up",
      SCOPE_SIGN_UP,
      DEFAULT_SIGN_UP_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/request-password-reset",
      SCOPE_PASSWORD_RESET,
      DEFAULT_PASSWORD_RESET_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/reset-password",
      SCOPE_PASSWORD_RESET,
      DEFAULT_PASSWORD_RESET_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/send-verification-email",
      SCOPE_VERIFY_EMAIL,
      DEFAULT_VERIFY_EMAIL_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/verify-email",
      SCOPE_VERIFY_EMAIL,
      DEFAULT_VERIFY_EMAIL_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/two-factor/verify-totp",
      SCOPE_TOTP,
      DEFAULT_TOTP_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/two-factor/verify-backup-code",
      SCOPE_BACKUP_CODE,
      DEFAULT_BACKUP_CODE_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/pedido/status-proc",
      SCOPE_PEDIDO_POLL,
      DEFAULT_PEDIDO_POLL_PER_MIN,
    ))
    .layer(
      RateLimitLayer::scoped(
        "/pedido",
        SCOPE_PEDIDO_PAGE,
        DEFAULT_PEDIDO_PAGE_PER_MIN,
      )
      .with_skips(&["/pedido/status-proc"]),
    )
    .layer(lindaflor::logging::layer::RequestLoggingLayer::noop());
  // APP_ENV is forced to development here, so HSTS stays off.
  lindaflor::security_headers::apply_security_header_tower_layers(builder)
    .build()
}

pub fn unique_email(prefix: &str) -> String {
  format!("{prefix}-{}@example.com", Uuid::now_v7().simple())
}

pub async fn body_bytes(response: Response<Body>) -> Bytes {
  response
    .into_body()
    .collect()
    .await
    .expect("collect body")
    .to_bytes()
}

pub async fn body_json(response: Response<Body>) -> serde_json::Value {
  let status = response.status();
  let bytes = body_bytes(response).await;
  if bytes.is_empty() {
    return serde_json::Value::Null;
  }
  match serde_json::from_slice(&bytes) {
    Ok(value) => value,
    Err(_) => {
      let text = String::from_utf8_lossy(&bytes).into_owned();
      serde_json::json!({
          "_non_json": true,
          "_status": status.as_u16(),
          "_body": text,
      })
    }
  }
}

/// Parse `session=<token>` from Set-Cookie (development cookie name).
pub fn session_cookie_from(response: &Response<Body>) -> Option<String> {
  for value in response.headers().get_all(header::SET_COOKIE) {
    let Ok(raw) = value.to_str() else {
      continue;
    };
    for part in raw.split(',') {
      let part = part.trim();
      if let Some(rest) = part.strip_prefix(&format!("{SESSION_COOKIE}=")) {
        let token = rest.split(';').next().unwrap_or(rest).trim();
        if !token.is_empty() && token != "\"\"" {
          return Some(format!("{SESSION_COOKIE}={token}"));
        }
      }
    }
  }
  None
}

pub struct JsonResponse {
  pub status: StatusCode,

  pub cookie: Option<String>,
  pub json: serde_json::Value,
  /// Merged cookie to send on the next request (request cookie overwritten by Set-Cookie).
  pub next_cookie: Option<String>,
}

pub async fn request(
  router: &Router,
  method: Method,
  path: &str,
  cookie: Option<&str>,
  body: Option<serde_json::Value>,
) -> JsonResponse {
  let mut builder = Request::builder().method(method).uri(path);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }

  let req = if let Some(body) = body {
    builder
      .header(header::CONTENT_TYPE, "application/json")
      .body(Body::from(
        serde_json::to_vec(&body).expect("serialize body"),
      ))
      .expect("build request")
  } else {
    builder.body(Body::empty()).expect("build request")
  };

  let response = router.handle(req).await;
  let status = response.status();
  let set_cookie = session_cookie_from(&response);
  let next_cookie = set_cookie.clone().or_else(|| cookie.map(str::to_owned));
  let json = body_json(response).await;

  JsonResponse {
    status,
    cookie: set_cookie,
    json,
    next_cookie,
  }
}

pub async fn json_post(
  router: &Router,
  path: &str,
  cookie: Option<&str>,
  body: serde_json::Value,
) -> JsonResponse {
  request(router, Method::POST, path, cookie, Some(body)).await
}

pub async fn json_get(
  router: &Router,
  path: &str,
  cookie: Option<&str>,
) -> JsonResponse {
  request(router, Method::GET, path, cookie, None).await
}

pub async fn sign_up(
  router: &Router,
  name: &str,
  email: &str,
  password: &str,
) -> JsonResponse {
  json_post(
    router,
    "/api/auth/sign-up/email",
    None,
    serde_json::json!({
        "name": name,
        "email": email,
        "password": password,
    }),
  )
  .await
}

pub async fn sign_in(
  router: &Router,
  email: &str,
  password: &str,
) -> JsonResponse {
  json_post(
    router,
    "/api/auth/sign-in/email",
    None,
    serde_json::json!({
        "email": email,
        "password": password,
    }),
  )
  .await
}

/// After an API call that stored only `value_hash`, plant a known raw token
/// on the newest matching row and return that secret.
///
/// `verifications.value` is empty at rest; never treat the stored hash as
/// the one-time secret.
pub async fn verification_token(
  pool: &PgPool,
  identifier_prefix: &str,
) -> String {
  use sqlx::Row;

  let token = lindaflor::auth::routes::dto::random_token();
  let token_hash = lindaflor::auth::service::verification_token_hash(&token);
  let like = format!("{identifier_prefix}%");
  let updated = sqlx::query(
    r#"
        UPDATE verifications
        SET value = '', value_hash = $1, updated_at = now()
        WHERE id = (
          SELECT id FROM verifications
          WHERE identifier LIKE $2
          ORDER BY created_at DESC
          LIMIT 1
        )
        RETURNING value, value_hash
        "#,
  )
  .bind(&token_hash)
  .bind(&like)
  .fetch_optional(pool)
  .await
  .expect("plant verification token")
  .expect("verification token row");

  let stored_value: String = updated.get("value");
  let stored_hash: Option<String> = updated.get("value_hash");
  assert!(
    stored_value.is_empty(),
    "raw token must not be stored in value"
  );
  assert_eq!(stored_hash.as_deref(), Some(token_hash.as_str()));
  token
}

pub fn totp_code(secret_b32: &str) -> String {
  use totp_rs::{Algorithm, Builder, Secret};

  let secret = Secret::try_from_base32(secret_b32).expect("base32 secret");
  let totp = Builder::new()
    .with_algorithm(Algorithm::SHA1)
    .with_digits(6)
    .with_skew(1)
    .with_step_duration(30)
    .with_secret(secret)
    .build()
    .expect("totp builder");
  totp.generate_current().to_string()
}
