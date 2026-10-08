use std::sync::{Arc, Mutex};

use http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use lindaflor::logging::layer::RequestLoggingLayer;
use lindaflor::logging::log_entry::{RequestLogEntry, SampleReason};
use lindaflor::logging::sink::LogSink;
use sqlx::PgPool;
use test_support::{body_bytes, pool_and_router, sign_up, unique_email};
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::router::{Body, Router, RouterBuilderDiscoverExt};
use topcoat::session::RouterBuilderSessionExt;
use uuid::Uuid;

fn collector() -> (LogSink, Arc<Mutex<Vec<RequestLogEntry>>>) {
  let entries = Arc::new(Mutex::new(Vec::new()));
  let sink_entries = Arc::clone(&entries);
  let sink: LogSink = Arc::new(move |entry| {
    sink_entries.lock().expect("collector").push(entry);
  });
  (sink, entries)
}

fn snapshot(
  entries: &Arc<Mutex<Vec<RequestLogEntry>>>,
) -> Vec<RequestLogEntry> {
  entries.lock().expect("collector").clone()
}

fn rate_limit_layers(
  builder: topcoat::router::RouterBuilder,
) -> topcoat::router::RouterBuilder {
  use lindaflor::rate_limit::{
    DEFAULT_BACKUP_CODE_PER_MIN, DEFAULT_GLOBAL_PER_MIN,
    DEFAULT_PASSWORD_RESET_PER_MIN, DEFAULT_PEDIDO_PAGE_PER_MIN,
    DEFAULT_PEDIDO_POLL_PER_MIN, DEFAULT_SIGN_IN_PER_MIN,
    DEFAULT_SIGN_UP_PER_MIN, DEFAULT_TOTP_PER_MIN,
    DEFAULT_VERIFY_EMAIL_PER_MIN, RateLimitLayer, SCOPE_BACKUP_CODE,
    SCOPE_PASSWORD_RESET, SCOPE_PEDIDO_PAGE, SCOPE_PEDIDO_POLL, SCOPE_SIGN_IN,
    SCOPE_SIGN_UP, SCOPE_TOTP, SCOPE_VERIFY_EMAIL,
  };
  builder
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
}

fn router_with_logging(pool: PgPool, layer: RequestLoggingLayer) -> Router {
  unsafe { std::env::set_var("APP_ENV", "development") };
  let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
  lindaflor::auth::routes::link_for_discover();
  let _ = lindaflor::api::health;
  let _ = lindaflor::logging::metrics_route::metrics;
  let _ = lindaflor::app::dashboard::page;
  let _ = lindaflor::app::login::page;

  rate_limit_layers(
    Router::builder()
      .discover()
      .cookies()
      .sessions(lindaflor::auth::session_config())
      .app_context(pool),
  )
  .layer(layer)
  .build()
}

async fn send(
  router: &Router,
  method: Method,
  path: &str,
  cookie: Option<&str>,
) -> http::Response<Body> {
  let mut builder = Request::builder().method(method).uri(path);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }
  router
    .handle(builder.body(Body::empty()).expect("request"))
    .await
}

#[tokio::test]
async fn health_logs_correlated_request_id() {
  let (pool, _) = pool_and_router().await;
  let (sink, entries) = collector();
  let router =
    router_with_logging(pool, RequestLoggingLayer::new(1.0, 1000, sink));

  let response = send(&router, Method::GET, "/api/health?foo=bar", None).await;
  assert_eq!(response.status(), StatusCode::OK);
  let header_id = response
    .headers()
    .get("x-request-id")
    .and_then(|v| v.to_str().ok())
    .expect("x-request-id");
  let request_id = Uuid::parse_str(header_id).expect("uuid");
  assert_eq!(request_id.get_version_num(), 7);

  let logged = snapshot(&entries);
  assert_eq!(logged.len(), 1);
  let entry = &logged[0];
  assert_eq!(entry.request_id, request_id);
  assert_eq!(entry.method, "GET");
  assert_eq!(entry.path, "/api/health");
  assert!(!entry.path.contains('?'));
  assert_eq!(entry.status, 200);
  assert!(entry.sampled);
  assert_eq!(entry.sample_reason, SampleReason::Sampled);
}

#[tokio::test]
async fn options_and_metrics_are_silent() {
  let (pool, _) = pool_and_router().await;
  let (sink, entries) = collector();
  let router =
    router_with_logging(pool, RequestLoggingLayer::new(1.0, 1000, sink));

  let options = send(&router, Method::OPTIONS, "/api/health", None).await;
  assert!(snapshot(&entries).is_empty(), "OPTIONS must not log");
  let _ = options;

  let response = send(&router, Method::GET, "/metrics", None).await;
  assert_eq!(response.status(), StatusCode::OK);
  let content_type = response
    .headers()
    .get(header::CONTENT_TYPE)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("");
  assert!(
    content_type.contains("0.0.4"),
    "content-type {content_type}"
  );
  let body = String::from_utf8_lossy(&body_bytes(response).await).into_owned();
  assert!(body.contains("http_requests_total") || body.contains("# TYPE"));
  assert!(
    !body.contains("route=\"/metrics\""),
    "scrape must not record /metrics: {body}"
  );
  let rendered = lindaflor::logging::metrics::format_prometheus();
  assert!(
    !rendered.contains("route=\"/metrics\""),
    "scrape must not record /metrics: {rendered}"
  );
  assert!(snapshot(&entries).is_empty());
}

#[tokio::test]
async fn not_found_is_kept_as_error() {
  let (pool, _) = pool_and_router().await;
  let (sink, entries) = collector();
  let router =
    router_with_logging(pool, RequestLoggingLayer::new(1.0, 1000, sink));

  let response = send(&router, Method::GET, "/no-such-route", None).await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
  assert!(response.headers().get("x-request-id").is_some());
  let logged = snapshot(&entries);
  assert_eq!(logged.len(), 1);
  assert_eq!(logged[0].status, 404);
  assert_eq!(logged[0].sample_reason, SampleReason::Error);
}

#[tokio::test]
async fn sample_rate_zero_drops_ok_requests() {
  let (pool, _) = pool_and_router().await;
  let (sink, entries) = collector();
  let router =
    router_with_logging(pool, RequestLoggingLayer::new(0.0, 60_000, sink));

  let response = send(&router, Method::GET, "/api/health", None).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert!(snapshot(&entries).is_empty());
}

#[tokio::test]
async fn signed_in_request_enriches_user_fields() {
  let (pool, _) = pool_and_router().await;
  let (sink, entries) = collector();
  let router =
    router_with_logging(pool, RequestLoggingLayer::new(1.0, 1000, sink));

  let email = unique_email("log");
  let signed = sign_up(&router, "Log User", &email, "password12").await;
  assert_eq!(signed.status, StatusCode::OK, "{:?}", signed.json);
  let cookie = signed.next_cookie.expect("session cookie");

  entries.lock().expect("collector").clear();
  let response =
    send(&router, Method::GET, "/api/auth/get-session", Some(&cookie)).await;
  assert_eq!(response.status(), StatusCode::OK);
  let _ = response.into_body().collect().await;

  let logged = snapshot(&entries);
  let auth = logged
    .iter()
    .find(|e| e.path == "/api/auth/get-session")
    .expect("get-session log");
  assert!(auth.user_id.is_some(), "{auth:?}");
  assert!(auth.session_id.is_some(), "{auth:?}");
}
