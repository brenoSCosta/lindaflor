//! Security response headers (PRODUCTION_READINESS §3.4).
//!
//! [`tower_http::set_header::SetResponseHeaderLayer`] (via
//! [`topcoat::router::tower::TowerLayer`]) is the primary mechanism: it
//! rewrites the response as it flows back through the middleware. Tower
//! middleware only sees `Ok` responses, so a [`SecurityHeadersFallbackLayer`]
//! (native Topcoat layer) queues the same values through
//! [`response_headers`](topcoat::router::response::response_headers)
//! *before* `next.run`, which the router applies even when the chain ends in
//! an error (Topcoat `Err`/404 renders). On success both fire with identical
//! values; on errors only the fallback lands.

use topcoat::context::Cx;
use topcoat::router::{
  Body, HeaderName, HeaderValue, Layer, LayerFuture, Next, Path, RouterBuilder,
  header,
  response::response_headers,
  tower::{TowerLayer, TowerNext},
};
use tower_http::set_header::{SetResponseHeader, SetResponseHeaderLayer};

/// Enforcing Content-Security-Policy. Allows the Topcoat runtime, theme
/// scripts, Tailwind stylesheet, toasts (inline scripts/styles) plus the
/// dev-docs CDN (Scalar/Swagger, dev-only routes).
///
/// `img-src` includes `http:` so local object storage (RustFS at
/// `http://127.0.0.1:4203`) can serve avatars and product images. Google
/// profile photos are `https:`. On an HTTPS origin the browser still
/// blocks mixed-content `http:` images, so production S3 must be HTTPS.
pub const CONTENT_SECURITY_POLICY_VALUE: &str = "default-src 'self'; script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net https://unpkg.com; style-src 'self' 'unsafe-inline' https://unpkg.com; img-src 'self' data: blob: http: https:; font-src 'self' data:; connect-src 'self' ws: wss:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

/// Prod-only `Strict-Transport-Security` (two years, subdomains).
pub const STRICT_TRANSPORT_SECURITY_VALUE: &str =
  "max-age=63072000; includeSubDomains";

pub fn content_type_options_value() -> HeaderValue {
  HeaderValue::from_static("nosniff")
}

pub fn referrer_policy_value() -> HeaderValue {
  HeaderValue::from_static("strict-origin-when-cross-origin")
}

pub fn content_security_policy_value() -> HeaderValue {
  HeaderValue::from_str(CONTENT_SECURITY_POLICY_VALUE)
    .expect("static CSP is a valid header value")
}

pub fn strict_transport_security_value() -> HeaderValue {
  HeaderValue::from_static(STRICT_TRANSPORT_SECURITY_VALUE)
}

/// HSTS applies everywhere except local/test environments.
pub fn hsts_enabled_for_env(app_env: &str) -> bool {
  !matches!(app_env, "development" | "dev" | "test")
}

/// HSTS gate on [`crate::config::app_env`].
pub fn hsts_enabled() -> bool {
  hsts_enabled_for_env(crate::config::app_env().as_str())
}

/// One tower `SetResponseHeader` layer (overriding mode).
pub type SecurityHeaderTowerLayer =
  TowerLayer<SetResponseHeader<TowerNext, HeaderValue>>;

pub fn set_response_header_layer(
  name: HeaderName,
  value: HeaderValue,
) -> SecurityHeaderTowerLayer {
  TowerLayer::new(SetResponseHeaderLayer::overriding(name, value))
}

/// Always-on tower layers: `X-Content-Type-Options`, `Referrer-Policy`, CSP.
pub fn always_tower_layers() -> Vec<SecurityHeaderTowerLayer> {
  vec![
    set_response_header_layer(
      header::X_CONTENT_TYPE_OPTIONS,
      content_type_options_value(),
    ),
    set_response_header_layer(header::REFERRER_POLICY, referrer_policy_value()),
    set_response_header_layer(
      header::CONTENT_SECURITY_POLICY,
      content_security_policy_value(),
    ),
  ]
}

/// Tower layers for an explicit env name (pure; no env read). Used by
/// [`tower_layers`] and unit tests.
pub fn tower_layers_for_env(app_env: &str) -> Vec<SecurityHeaderTowerLayer> {
  let mut layers = always_tower_layers();
  if hsts_enabled_for_env(app_env) {
    layers.push(set_response_header_layer(
      header::STRICT_TRANSPORT_SECURITY,
      strict_transport_security_value(),
    ));
  }
  layers
}

/// Tower layers for the current process env (HSTS unless dev/dev/test).
pub fn tower_layers() -> Vec<SecurityHeaderTowerLayer> {
  tower_layers_for_env(crate::config::app_env().as_str())
}

/// Native fallback so headers also land on Topcoat `Err`/404 renders, which
/// leave the tower chain as errors before middleware can stamp them. Queued
/// pre-`next.run` via `response_headers`; tower stays primary on success.
#[derive(Debug, Clone, Copy, Default)]
pub struct SecurityHeadersFallbackLayer;

impl Layer for SecurityHeadersFallbackLayer {
  fn path(&self) -> Option<&Path> {
    None
  }

  fn handle<'a>(
    &'a self,
    cx: &'a Cx,
    body: Body,
    next: Next<'a>,
  ) -> LayerFuture<'a> {
    Box::pin(async move {
      let slot = response_headers(cx);
      slot.append(header::X_CONTENT_TYPE_OPTIONS, content_type_options_value());
      slot.append(header::REFERRER_POLICY, referrer_policy_value());
      slot.append(
        header::CONTENT_SECURITY_POLICY,
        content_security_policy_value(),
      );
      if hsts_enabled() {
        slot.append(
          header::STRICT_TRANSPORT_SECURITY,
          strict_transport_security_value(),
        );
      }
      next.run(cx, body).await
    })
  }
}

/// Queue headers before inner layers (covers Topcoat `Err`/404 and 429s
/// that never reach tower). Register this *before* RateLimit so a 429
/// still inherits the queued values. OriginLayer stays outside all of this.
pub fn apply_security_headers_fallback(
  builder: RouterBuilder,
) -> RouterBuilder {
  builder.layer(SecurityHeadersFallbackLayer)
}

/// Tower `SetResponseHeader` layers (HSTS only outside dev/dev/test).
/// Register last so they wrap Ok responses as they unwind.
pub fn apply_security_header_tower_layers(
  mut builder: RouterBuilder,
) -> RouterBuilder {
  for layer in tower_layers() {
    builder = builder.layer(layer);
  }
  builder
}

/// Fallback then tower (no layers in between). Fine for tests that do not
/// need RateLimit between the two; production wiring splits them so 429s
/// still get headers.
pub fn apply_security_headers(builder: RouterBuilder) -> RouterBuilder {
  apply_security_header_tower_layers(apply_security_headers_fallback(builder))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn header_values_match_spec() {
    assert_eq!(content_type_options_value(), "nosniff");
    assert_eq!(referrer_policy_value(), "strict-origin-when-cross-origin");
    assert_eq!(
      content_security_policy_value().to_str().unwrap(),
      CONTENT_SECURITY_POLICY_VALUE
    );
    assert!(content_security_policy_value().to_str().unwrap().contains(
      "script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net https://unpkg.com"
    ));
    assert!(
      content_security_policy_value()
        .to_str()
        .unwrap()
        .contains("img-src 'self' data: blob: http: https:"),
      "local HTTP object-store URLs (RustFS) must be allowed"
    );
    assert_eq!(
      strict_transport_security_value(),
      "max-age=63072000; includeSubDomains"
    );
  }

  #[test]
  fn hsts_gate_skips_dev_variants_only() {
    for env in ["development", "dev", "test"] {
      assert!(!hsts_enabled_for_env(env), "{env} must skip HSTS");
      assert_eq!(
        tower_layers_for_env(env).len(),
        3,
        "{env} must have exactly the always-on layers"
      );
    }
    for env in ["production", "staging", "prod"] {
      assert!(hsts_enabled_for_env(env), "{env} must include HSTS");
      assert_eq!(tower_layers_for_env(env).len(), 4);
    }
  }

  #[test]
  fn prod_includes_hsts_layer() {
    // 3 always-on layers + HSTS.
    assert_eq!(tower_layers_for_env("production").len(), 4);
    assert_eq!(
      strict_transport_security_value().to_str().unwrap(),
      STRICT_TRANSPORT_SECURITY_VALUE
    );
  }
}
