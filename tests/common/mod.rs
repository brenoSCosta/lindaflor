#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode, header};
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::OnceCell;
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::router::{Body, Router, RouterBuilderDiscoverExt};
use topcoat::session::RouterBuilderSessionExt;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "session";

#[path = "../../src/embedded_postgres.rs"]
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

      // Force inventory discover to see library routes.
      let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
      lindaflor::auth::routes::link_for_discover();
      let _ = lindaflor::api::health;

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
      sqlx::migrate!("./migrations")
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

/// Fresh API-only router (avoids cross-test cookie/session residue on a shared Router).
pub fn router(pool: &PgPool) -> Router {
  build_auth_router(pool.clone())
}

pub fn build_auth_router(pool: PgPool) -> Router {
  unsafe { std::env::set_var("APP_ENV", "development") };
  let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
  lindaflor::auth::routes::link_for_discover();
  let _ = lindaflor::api::health;

  Router::builder()
    .discover()
    .cookies()
    .sessions(lindaflor::auth::session_config())
    .app_context(pool)
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

pub async fn verification_token(
  pool: &PgPool,
  identifier_prefix: &str,
) -> String {
  let row = sqlx::query!(
    r#"
        SELECT value
        FROM verifications
        WHERE identifier LIKE $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    format!("{identifier_prefix}%"),
  )
  .fetch_one(pool)
  .await
  .expect("verification token row");
  row.value
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
