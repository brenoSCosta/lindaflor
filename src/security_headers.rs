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
/// `'unsafe-eval'` is required: Topcoat compiles live expressions with
/// `new Function` in the browser. Without it, field bindings never hydrate.
///
/// `img-src` includes `http:` so local object storage (RustFS at
/// `http://127.0.0.1:4203`) can serve avatars and product images. Google
/// profile photos are `https:`. On an HTTPS origin the browser still
/// blocks mixed-content `http:` images, so production S3 must be HTTPS.
pub const CONTENT_SECURITY_POLICY_VALUE: &str = "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' https://cdn.jsdelivr.net https://unpkg.com; style-src 'self' 'unsafe-inline' https://unpkg.com; img-src 'self' data: blob: http: https:; font-src 'self' data:; connect-src 'self' ws: wss:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

/// Dev-only CSP. `topcoat::dev::script()` (see `src/app.rs` root layout)
/// injects `<script src="{TOPCOAT_DEV_URL}/dev.js">` when running under
/// `topcoat dev`, where `TOPCOAT_DEV_URL` is `http://127.0.0.1:<ephemeral-port>`
/// (e.g. `http://127.0.0.1:59039`). The port changes every `topcoat dev`
/// run, so `script-src`/`style-src`/`connect-src` allow any local port via
/// `http://127.0.0.1:*` and `http://localhost:*` (CSP port wildcards).
/// This also covers RustFS on `:4203`. CDN hosts are kept so the dev-only
/// Scalar/Swagger routes (`/api/docs`, `/api/swagger`) keep working.
/// `connect-src` keeps `ws:`/`wss:` for the dev reload WebSocket plus local
/// `http:` for HMR fetches. `font-src`/`style-src` allow jsDelivr because
/// `dev.js` loads its status-indicator font (Lexend Deca via fontsource)
/// and stylesheet from there.
pub const CONTENT_SECURITY_POLICY_VALUE_DEV: &str = "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' http://127.0.0.1:* http://localhost:* https://cdn.jsdelivr.net https://unpkg.com; style-src 'self' 'unsafe-inline' http://127.0.0.1:* http://localhost:* https://cdn.jsdelivr.net https://unpkg.com; img-src 'self' data: blob: http: https:; font-src 'self' data: https://cdn.jsdelivr.net https://unpkg.com; connect-src 'self' ws: wss: http://127.0.0.1:* http://localhost:*; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

/// Prod-only `Strict-Transport-Security` (two years, subdomains).
pub const STRICT_TRANSPORT_SECURITY_VALUE: &str =
  "max-age=63072000; includeSubDomains";

pub fn content_type_options_value() -> HeaderValue {
  HeaderValue::from_static("nosniff")
}

pub fn referrer_policy_value() -> HeaderValue {
  HeaderValue::from_static("strict-origin-when-cross-origin")
}

/// Local/test only: `topcoat dev` + test routers. Unknown envs (staging,
/// preview, typos) stay on the prod CSP — same fail-closed list as HSTS.
pub fn dev_csp_for_env(app_env: &str) -> bool {
  matches!(app_env, "development" | "dev" | "test")
}

pub fn content_security_policy_value_for_env(app_env: &str) -> HeaderValue {
  let value = if dev_csp_for_env(app_env) {
    CONTENT_SECURITY_POLICY_VALUE_DEV
  } else {
    CONTENT_SECURITY_POLICY_VALUE
  };
  HeaderValue::from_str(value).expect("static CSP is a valid header value")
}

pub fn content_security_policy_value() -> HeaderValue {
  content_security_policy_value_for_env(crate::config::app_env().as_str())
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

/// Always-on tower layers for an explicit env (pure; no env read).
pub fn always_tower_layers_for_env(
  app_env: &str,
) -> Vec<SecurityHeaderTowerLayer> {
  vec![
    set_response_header_layer(
      header::X_CONTENT_TYPE_OPTIONS,
      content_type_options_value(),
    ),
    set_response_header_layer(header::REFERRER_POLICY, referrer_policy_value()),
    set_response_header_layer(
      header::CONTENT_SECURITY_POLICY,
      content_security_policy_value_for_env(app_env),
    ),
  ]
}

/// Always-on tower layers: `X-Content-Type-Options`, `Referrer-Policy`, CSP.
pub fn always_tower_layers() -> Vec<SecurityHeaderTowerLayer> {
  always_tower_layers_for_env(crate::config::app_env().as_str())
}

/// Tower layers for an explicit env name (pure; no env read). Used by
/// [`tower_layers`] and unit tests.
pub fn tower_layers_for_env(app_env: &str) -> Vec<SecurityHeaderTowerLayer> {
  let mut layers = always_tower_layers_for_env(app_env);
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
      content_security_policy_value_for_env("production")
        .to_str()
        .unwrap(),
      CONTENT_SECURITY_POLICY_VALUE
    );
    assert!(
      content_security_policy_value_for_env("production")
        .to_str()
        .unwrap()
        .contains(
          "script-src 'self' 'unsafe-inline' 'unsafe-eval' https://cdn.jsdelivr.net https://unpkg.com"
        )
    );
    assert!(
      content_security_policy_value_for_env("production")
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
  fn dev_csp_allows_topcoat_dev_server() {
    // `topcoat dev` serves dev.js from an ephemeral port
    // (TOPCOAT_DEV_URL=http://127.0.0.1:<random>), so the dev CSP must
    // allow any local port — a pinned port (e.g. :4203) blocks dev.js.
    for env in ["development", "dev", "test"] {
      let csp = content_security_policy_value_for_env(env)
        .to_str()
        .unwrap()
        .to_owned();
      assert!(dev_csp_for_env(env), "{env} must enable the local CSP");
      assert_eq!(
        csp, CONTENT_SECURITY_POLICY_VALUE_DEV,
        "{env} must use dev CSP"
      );
      assert!(
        csp.contains("http://127.0.0.1:*"),
        "{env} CSP must allow ephemeral topcoat dev port, got: {csp}"
      );
      assert!(
        csp.contains("http://localhost:*"),
        "{env} CSP must allow localhost dev port, got: {csp}"
      );
      assert!(
        csp.contains("https://cdn.jsdelivr.net"),
        "{env} CSP must keep dev-docs CDN (Scalar), got: {csp}"
      );
      assert!(
        csp.contains("https://unpkg.com"),
        "{env} CSP must keep dev-docs CDN (Swagger), got: {csp}"
      );
      assert!(
        csp.contains("font-src 'self' data: https://cdn.jsdelivr.net"),
        "{env} CSP font-src must allow dev.js status font (Lexend Deca via fontsource), got: {csp}"
      );
    }
  }

  #[test]
  fn csp_gate_is_fail_closed_like_hsts() {
    for env in ["production", "staging", "prod", "preview"] {
      let csp = content_security_policy_value_for_env(env)
        .to_str()
        .unwrap()
        .to_owned();
      assert!(!dev_csp_for_env(env), "{env} must not enable the local CSP");
      assert_eq!(
        csp, CONTENT_SECURITY_POLICY_VALUE,
        "{env} must use prod CSP"
      );
      assert!(
        !csp.contains("http://127.0.0.1:*")
          && !csp.contains("http://localhost:*"),
        "{env} must not allow localhost script/connect wildcards, got: {csp}"
      );
    }
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
