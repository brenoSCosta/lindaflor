use std::time::Duration;

use http::StatusCode;
use sqlx::postgres::PgPoolOptions;
use test_support::{env, json_get};
use topcoat::cookie::RouterBuilderCookieExt;
use topcoat::router::{Router, RouterBuilderDiscoverExt};
use topcoat::session::RouterBuilderSessionExt;

use lindaflor::rate_limit::RateLimitLayer;

/// Scoped paths own their requests: the global layer must skip them.
static HEALTH_SKIPS: &[&str] = &["/api/health"];
/// Unique scope so governor buckets can't collide with other suites.
const TEST_SCOPED: &str = "test:rate-limit";

/// Global default applies everywhere else; a scoped layer *replaces* it
/// on its path instead of stacking (single budget consumed per request).
#[tokio::test]
async fn global_default_with_scoped_override() {
  let test_env = env().await;
  // `env()` installs generous `RATE_LIMIT_*` budgets for other suites.
  // Tighten global *before the first request* (governor quotas are created
  // lazily per scope on first use).
  unsafe { std::env::set_var("RATE_LIMIT_GLOBAL_PER_MIN", "2") };
  let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
  lindaflor::auth::routes::link_for_discover();
  let _ = lindaflor::api::health;

  let pool = PgPoolOptions::new()
    .max_connections(5)
    .acquire_timeout(Duration::from_secs(10))
    .connect(&test_env.database_url)
    .await
    .expect("connect pool");
  let router = Router::builder()
    .discover()
    .cookies()
    .sessions(lindaflor::auth::session_config())
    .app_context(pool)
    .layer(RateLimitLayer::global(2).with_skips(HEALTH_SKIPS))
    .layer(RateLimitLayer::scoped("/api/health", TEST_SCOPED, 100))
    .build();

  // Scoped path: global skipped, scoped budget (100) owns it → all pass.
  // (Stacking would 429 the 3rd request against the global quota of 2.)
  for _ in 0..3 {
    let response = json_get(&router, "/api/health", None).await;
    assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
  }

  // Unlisted path: global budget (2) applies → third request is rejected.
  // Pathless layers also wrap 404/405, so a missing route still counts.
  for (i, expected) in [
    StatusCode::NOT_FOUND,
    StatusCode::NOT_FOUND,
    StatusCode::TOO_MANY_REQUESTS,
  ]
  .iter()
  .enumerate()
  {
    let response =
      json_get(&router, "/no-such-route-for-rate-limit", None).await;
    assert_eq!(
      response.status,
      *expected,
      "request {} to unlisted path",
      i + 1
    );
  }
}
