//! PRODUCTION_READINESS §3.4: security headers + same-origin enforcement.
//!
//! Uses raw `http::Request` (the `json_get`/`json_post` helpers cannot set
//! headers). `pool_and_router` forces `APP_ENV=development`, so HSTS must be
//! absent here; prod inclusion is covered by unit tests in
//! `src/security_headers.rs` / `src/config.rs` (no env mutation here, so no
//! parallel races beyond the shared `development` value).

use http::{Method, Request, StatusCode, header};
use test_support::pool_and_router;
use topcoat::router::{Body, Router};

use lindaflor::security_headers::{
  CONTENT_SECURITY_POLICY_VALUE_DEV, STRICT_TRANSPORT_SECURITY_VALUE,
};

async fn send(
  router: &Router,
  method: Method,
  path: &str,
  headers: &[(&str, &str)],
  body: Option<serde_json::Value>,
) -> http::Response<Body> {
  let mut builder = Request::builder().method(method).uri(path);
  for (name, value) in headers {
    builder = builder.header(*name, *value);
  }
  let request = if let Some(json) = body {
    builder
      .header(header::CONTENT_TYPE, "application/json")
      .body(Body::from(
        serde_json::to_vec(&json).expect("serialize body"),
      ))
      .expect("build request")
  } else {
    builder.body(Body::empty()).expect("build request")
  };
  router.handle(request).await
}

fn get_header(
  response: &http::Response<Body>,
  name: header::HeaderName,
) -> Option<&str> {
  response.headers().get(name).and_then(|v| v.to_str().ok())
}

/// Always-on headers on a 200, HSTS absent in the test env.
#[tokio::test]
async fn security_headers_present_on_ok() {
  let (_pool, router) = pool_and_router().await;
  let response = send(&router, Method::GET, "/api/health", &[], None).await;
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    get_header(&response, header::X_CONTENT_TYPE_OPTIONS),
    Some("nosniff")
  );
  assert_eq!(
    get_header(&response, header::REFERRER_POLICY),
    Some("strict-origin-when-cross-origin")
  );
  // `pool_and_router` forces APP_ENV=development, so the dev CSP (with the
  // `http://127.0.0.1:*` allowance for `topcoat dev`'s ephemeral dev.js)
  // is expected here; prod CSP is covered by unit tests.
  assert_eq!(
    get_header(&response, header::CONTENT_SECURITY_POLICY),
    Some(CONTENT_SECURITY_POLICY_VALUE_DEV)
  );
  assert_eq!(
    get_header(&response, header::STRICT_TRANSPORT_SECURITY),
    None,
    "HSTS must be absent when APP_ENV=development, got {:?}",
    STRICT_TRANSPORT_SECURITY_VALUE
  );
}

/// The native `response_headers` fallback must stamp Topcoat Err renders
/// (tower middleware only sees Ok responses): 404 and handler 401s carry
/// the same headers.
#[tokio::test]
async fn security_headers_present_on_404_and_handler_error() {
  let (_pool, router) = pool_and_router().await;

  let not_found =
    send(&router, Method::GET, "/no-such-route-3-4", &[], None).await;
  assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
  assert_eq!(
    get_header(&not_found, header::X_CONTENT_TYPE_OPTIONS),
    Some("nosniff")
  );
  assert_eq!(
    get_header(&not_found, header::CONTENT_SECURITY_POLICY),
    Some(CONTENT_SECURITY_POLICY_VALUE_DEV)
  );
  assert_eq!(
    get_header(&not_found, header::STRICT_TRANSPORT_SECURITY),
    None,
    "HSTS must be absent when APP_ENV=development"
  );

  // 401 from a handler (not the origin layer) also flows through the chain.
  let unauthorized =
    send(&router, Method::GET, "/api/auth/list-accounts", &[], None).await;
  assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
  assert_eq!(
    get_header(&unauthorized, header::X_CONTENT_TYPE_OPTIONS),
    Some("nosniff")
  );
  assert_eq!(
    get_header(&unauthorized, header::CONTENT_SECURITY_POLICY),
    Some(CONTENT_SECURITY_POLICY_VALUE_DEV)
  );
}

/// Documents *why* the native fallback exists: a tower-only
/// `SetResponseHeader` layer stamps the 200 but misses the 404, because
/// Topcoat errors leave the tower chain as errors before middleware can
/// modify a response.
#[tokio::test]
async fn tower_only_layer_misses_error_renders() {
  use lindaflor::security_headers::set_response_header_layer;
  use topcoat::router::RouterBuilderDiscoverExt;

  let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
  lindaflor::auth::routes::link_for_discover();
  let _ = lindaflor::api::health;
  let router = Router::builder()
    .discover()
    .layer(set_response_header_layer(
      header::X_CONTENT_TYPE_OPTIONS,
      http::HeaderValue::from_static("nosniff"),
    ))
    .build();

  let ok = send(&router, Method::GET, "/api/health", &[], None).await;
  assert_eq!(ok.status(), StatusCode::OK);
  assert_eq!(
    get_header(&ok, header::X_CONTENT_TYPE_OPTIONS),
    Some("nosniff")
  );

  let missing =
    send(&router, Method::GET, "/no-such-route-3-4", &[], None).await;
  assert_eq!(missing.status(), StatusCode::NOT_FOUND);
  assert_eq!(
    get_header(&missing, header::X_CONTENT_TYPE_OPTIONS),
    None,
    "tower SetResponseHeader cannot stamp Topcoat Err/404 renders"
  );
}

/// Same-origin-only enforcement (default `OriginPolicy`, no trusted
/// origins in tests): cross-site POSTs are rejected with 403, while a
/// same-origin POST reaches the handler (400 on the empty body, not 403).
#[tokio::test]
async fn cross_origin_post_rejected_same_origin_passes() {
  let (_pool, router) = pool_and_router().await;
  let path = "/api/auth/sign-up/email";

  let evil = send(
    &router,
    Method::POST,
    path,
    &[
      ("origin", "https://evil.test"),
      ("sec-fetch-site", "cross-site"),
    ],
    Some(serde_json::json!({})),
  )
  .await;
  assert_eq!(evil.status(), StatusCode::FORBIDDEN);

  let same_origin = send(
    &router,
    Method::POST,
    path,
    &[("sec-fetch-site", "same-origin")],
    Some(serde_json::json!({})),
  )
  .await;
  assert_ne!(
    same_origin.status(),
    StatusCode::FORBIDDEN,
    "same-origin POST must reach the handler"
  );
}

/// Fallback is registered outside RateLimit, so a 429 still carries
/// always-on headers. OriginPolicy 403s stay unstamped (OriginLayer is
/// outermost, before any application layer).
#[tokio::test]
async fn security_headers_present_on_429() {
  use lindaflor::rate_limit::RateLimitLayer;
  use topcoat::router::RouterBuilderDiscoverExt;

  let _ = lindaflor::auth::CREDENTIAL_PROVIDER_ID;
  lindaflor::auth::routes::link_for_discover();
  let _ = lindaflor::api::health;

  let router = {
    let builder = Router::builder().discover();
    let builder =
      lindaflor::security_headers::apply_security_headers_fallback(builder)
        .layer(RateLimitLayer::scoped(
          "/api/health",
          "test:security-headers-429",
          1,
        ));
    lindaflor::security_headers::apply_security_header_tower_layers(builder)
      .build()
  };

  let first = send(&router, Method::GET, "/api/health", &[], None).await;
  assert_eq!(first.status(), StatusCode::OK);

  let limited = send(&router, Method::GET, "/api/health", &[], None).await;
  assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
  assert_eq!(
    get_header(&limited, header::X_CONTENT_TYPE_OPTIONS),
    Some("nosniff")
  );
  assert_eq!(
    get_header(&limited, header::CONTENT_SECURITY_POLICY),
    Some(CONTENT_SECURITY_POLICY_VALUE_DEV)
  );
  assert_eq!(
    get_header(&limited, header::STRICT_TRANSPORT_SECURITY),
    None
  );
}
