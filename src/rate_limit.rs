//! In-process per-IP rate limits (governor) + Topcoat layer.
//!
//! Coarse DoS guard, not a security boundary. Security-critical counters
//! (auth failures, 2FA attempts, TOTP reuse, pending-token bindings) live
//! in [`crate::valkey`]. Layers are registered in `src/app.rs`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Mutex, OnceLock};

use governor::{
  Quota, RateLimiter, clock::DefaultClock, state::keyed::DashMapStateStore,
};

type IpLimiter = RateLimiter<String, DashMapStateStore<String>, DefaultClock>;

static LIMITERS: OnceLock<Mutex<HashMap<&'static str, IpLimiter>>> =
  OnceLock::new();

fn limiters() -> &'static Mutex<HashMap<&'static str, IpLimiter>> {
  LIMITERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn quota_per_minute(per_minute: u32) -> Quota {
  Quota::per_minute(
    NonZeroU32::new(per_minute.max(1)).expect("per_minute >= 1"),
  )
}

/// Env override `RATE_LIMIT_<SCOPE>_PER_MIN` (e.g. `RATE_LIMIT_AUTH_PER_MIN`),
/// else `default`. Scopes are the `SCOPE_*` constants below with `:`/`-`
/// normalized to `_` and uppercased.
pub fn limit_for(scope: &'static str, default: u32) -> u32 {
  let key = format!(
    "RATE_LIMIT_{}_PER_MIN",
    scope.replace([':', '-'], "_").to_ascii_uppercase()
  );
  std::env::var(&key)
    .ok()
    .and_then(|v| v.parse::<u32>().ok())
    .filter(|v| *v > 0)
    .unwrap_or(default)
}

/// Fixed scopes so governor buckets stay stable across calls.
pub const SCOPE_SIGN_IN: &str = "auth:sign-in";
pub const SCOPE_SIGN_UP: &str = "auth:sign-up";
pub const SCOPE_PASSWORD_RESET: &str = "auth:password-reset";
pub const SCOPE_VERIFY_EMAIL: &str = "auth:verify-email";
pub const SCOPE_TOTP: &str = "auth:verify-totp";
pub const SCOPE_BACKUP_CODE: &str = "auth:verify-backup-code";
pub const SCOPE_TWO_FACTOR: &str = "auth:two-factor";
pub const SCOPE_PEDIDO_POLL: &str = "pedido:poll";
pub const SCOPE_PEDIDO_PAGE: &str = "pedido:page";
pub const SCOPE_COUPON: &str = "coupon:apply";
pub const SCOPE_COUPON_CART: &str = "coupon:cart";
pub const SCOPE_MP_WEBHOOK: &str = "webhook:mercadopago";
pub const SCOPE_GLOBAL: &str = "global";

/// Default budgets: 5/min/IP on auth endpoints, 3/min on
/// TOTP/backup, 10/min on order-token poll, 30/min on order pages,
/// 600/min/IP global default on everything else.
pub const DEFAULT_SIGN_IN_PER_MIN: u32 = 5;
pub const DEFAULT_SIGN_UP_PER_MIN: u32 = 5;
pub const DEFAULT_PASSWORD_RESET_PER_MIN: u32 = 5;
pub const DEFAULT_VERIFY_EMAIL_PER_MIN: u32 = 5;
pub const DEFAULT_TOTP_PER_MIN: u32 = 3;
pub const DEFAULT_BACKUP_CODE_PER_MIN: u32 = 3;
pub const DEFAULT_TWO_FACTOR_PER_MIN: u32 = 10;
pub const DEFAULT_PEDIDO_POLL_PER_MIN: u32 = 10;
pub const DEFAULT_PEDIDO_PAGE_PER_MIN: u32 = 30;
/// Coupon apply: per IP (scoped layer) and per cart (in the procedure).
pub const DEFAULT_COUPON_PER_MIN: u32 = 10;
pub const DEFAULT_COUPON_CART_PER_MIN: u32 = 10;
/// Mercado Pago notifies from a handful of IPs, in bursts after outages.
pub const DEFAULT_MP_WEBHOOK_PER_MIN: u32 = 120;
pub const DEFAULT_GLOBAL_PER_MIN: u32 = 600;

/// Prefixes the global governor skips so those requests do not consume the
/// global budget. Includes scoped [`RateLimitLayer`] paths (true override,
/// not stacking), `/metrics` scrapes (no scoped layer) and bundled static
/// assets (`/_topcoat/assets/...`, served by Topcoat with immutable
/// content-hashed URLs — exempt so CGNAT/mobile networks sharing few IPs
/// don't burn the global budget on cacheable files). Keep in sync
/// with the scoped layers in `src/app.rs` (and `tests/common/mod.rs`).
pub const GLOBAL_OVERRIDE_PREFIXES: &[&str] = &[
  "/api/auth/sign-in",
  "/api/auth/sign-up",
  "/api/auth/request-password-reset",
  "/api/auth/reset-password",
  "/api/auth/send-verification-email",
  "/api/auth/verify-email",
  "/api/auth/two-factor/verify-totp",
  "/api/auth/two-factor/verify-backup-code",
  "/pedido/status-proc",
  "/pedido",
  "/api/webhooks/mercadopago",
  "/metrics",
  "/_topcoat/assets",
];

/// Like [`GLOBAL_OVERRIDE_PREFIXES`], but only for mutating requests
/// (anything but `GET`/`HEAD`/`OPTIONS`): these paths are owned by
/// [`RateLimitLayer::mutations_only`] layers (see [`apply_form_rate_limits`]),
/// so their `GET` pages keep consuming the global budget while the form
/// `POST`s consume exactly one (strict) scoped budget.
pub const GLOBAL_OVERRIDE_MUTATION_PREFIXES: &[&str] = &[
  "/forgot-password",
  "/reset-password",
  "/check-email/resend",
  "/verify-email/resend",
  "/two-factor",
  "/login/google",
  "/carrinho/cupom",
];

/// HTML-form twins of the `/api/auth/*` endpoints, plus coupon apply.
/// Same scopes (and therefore shared buckets) as the API versions, so a
/// client can't double its budget by switching between JSON and forms.
/// `(path, scope, default per minute)`; every path must also be listed in
/// [`GLOBAL_OVERRIDE_MUTATION_PREFIXES`].
pub const FORM_RATE_LIMITS: &[(&str, &str, u32)] = &[
  (
    "/forgot-password",
    SCOPE_PASSWORD_RESET,
    DEFAULT_PASSWORD_RESET_PER_MIN,
  ),
  (
    "/reset-password",
    SCOPE_PASSWORD_RESET,
    DEFAULT_PASSWORD_RESET_PER_MIN,
  ),
  (
    "/check-email/resend",
    SCOPE_VERIFY_EMAIL,
    DEFAULT_VERIFY_EMAIL_PER_MIN,
  ),
  (
    "/verify-email/resend",
    SCOPE_VERIFY_EMAIL,
    DEFAULT_VERIFY_EMAIL_PER_MIN,
  ),
  ("/two-factor", SCOPE_TOTP, DEFAULT_TOTP_PER_MIN),
  ("/login/google", SCOPE_SIGN_IN, DEFAULT_SIGN_IN_PER_MIN),
  ("/carrinho/cupom", SCOPE_COUPON, DEFAULT_COUPON_PER_MIN),
];

/// Register a [`RateLimitLayer::mutations_only`] layer for every entry of
/// [`FORM_RATE_LIMITS`]. Called from `src/app.rs` (and the test routers in
/// `crates/test-support`) so the lists can't drift apart.
pub fn apply_form_rate_limits(
  mut builder: topcoat::router::RouterBuilder,
) -> topcoat::router::RouterBuilder {
  for &(path, scope, per_minute) in FORM_RATE_LIMITS {
    builder = builder
      .layer(RateLimitLayer::scoped(path, scope, per_minute).mutations_only());
  }
  builder
}

fn is_mutation(method: &http::Method) -> bool {
  !matches!(
    *method,
    http::Method::GET | http::Method::HEAD | http::Method::OPTIONS
  )
}

/// Segment-aware prefix match (mirrors Topcoat layer matching): `path`
/// equals `prefix` or starts with `prefix + "/"`.
fn path_starts_with(path: &str, prefix: &str) -> bool {
  path == prefix
    || (path.starts_with(prefix)
      && path.as_bytes().get(prefix.len()) == Some(&b'/'))
}

/// Check (and consume) one cell of the `(scope, ip)` per-minute bucket.
/// Returns `true` when the request may proceed.
pub fn check_ip_rate(scope: &'static str, ip: &str, per_minute: u32) -> bool {
  let mut guard = limiters().lock().unwrap_or_else(|e| e.into_inner());
  let limiter = guard
    .entry(scope)
    .or_insert_with(|| IpLimiter::dashmap(quota_per_minute(per_minute)));
  limiter.check_key(&format!("{scope}:{ip}")).is_ok()
}

/// Drop governor entries whose buckets have fully replenished and shrink
/// the key maps back down. Call periodically (see the reservation sweep
/// loop in `src/main.rs`): without this, every distinct IP ever seen keeps
/// a `DashMap` entry for the process lifetime.
pub fn prune_limiters() {
  let guard = limiters().lock().unwrap_or_else(|e| e.into_inner());
  for limiter in guard.values() {
    limiter.retain_recent();
    limiter.shrink_to_fit();
  }
}

/// Generic 429 error (no oracle detail). Handlers return it with
/// `return Err(rate_limit::rate_limit_error())`.
pub fn rate_limit_error() -> topcoat::Error {
  topcoat::router::error::too_many_requests(60).into()
}

/// Topcoat [`topcoat::router::Layer`] enforcing a per-IP per-minute budget
/// on every matched route under `path` (prefix rule, same as `BodyLimit`).
/// Registered in `src/app.rs`, e.g.
///
/// ```rust,ignore
/// .layer(RateLimitLayer::scoped("/api/auth/sign-in", SCOPE_SIGN_IN, 5))
/// ```
///
/// Override model: the global layer ([`RateLimitLayer::global`]) skips paths
/// owned by scoped layers (see [`GLOBAL_OVERRIDE_PREFIXES`]), so each
/// request consumes exactly one budget — a scoped budget *replaces* the
/// global one instead of stacking on top of it.
pub struct RateLimitLayer {
  path: Option<Cow<'static, topcoat::router::Path>>,
  scope: &'static str,
  per_minute: u32,
  skip_prefixes: &'static [&'static str],
  /// Like `skip_prefixes`, but only for mutating requests.
  mutation_skip_prefixes: &'static [&'static str],
  /// Enforce only on mutating requests (`GET`/`HEAD`/`OPTIONS` pass).
  mutations_only: bool,
}

impl RateLimitLayer {
  /// Layer enforcing `per_minute` requests/IP on `path` and below.
  /// `path` must be a well-formed route path (panics otherwise, like
  /// `BodyLimit::at`); `scope` should be one of the `SCOPE_*` constants so
  /// buckets are shared with the in-handler checks.
  pub fn scoped(
    path: &'static str,
    scope: &'static str,
    per_minute: u32,
  ) -> Self {
    Self {
      path: Some(topcoat::router::IntoPath::into_path(path)),
      scope,
      per_minute,
      skip_prefixes: &[],
      mutation_skip_prefixes: &[],
      mutations_only: false,
    }
  }

  /// Global default budget for every route not owned by a scoped layer.
  /// Pathless (`None`), so it wraps every request including 404/405 and
  /// static asset routes (see Topcoat `builder.rs` layer semantics).
  /// Skips the [`GLOBAL_OVERRIDE_PREFIXES`] so scoped layers own those
  /// requests and `/metrics` scrapes skip the global governor.
  pub fn global(per_minute: u32) -> Self {
    Self {
      path: None,
      scope: SCOPE_GLOBAL,
      per_minute,
      skip_prefixes: GLOBAL_OVERRIDE_PREFIXES,
      mutation_skip_prefixes: GLOBAL_OVERRIDE_MUTATION_PREFIXES,
      mutations_only: false,
    }
  }

  /// Skip enforcement when the request path falls under any of `prefixes`
  /// (segment-aware). Used for true-override semantics, e.g. the `/pedido`
  /// page layer skips `/pedido/status-proc` which has its own tighter scope.
  pub fn with_skips(mut self, prefixes: &'static [&'static str]) -> Self {
    self.skip_prefixes = prefixes;
    self
  }

  /// Only enforce on mutating requests (form `POST`s, procedures), so a
  /// strict budget on e.g. `POST /two-factor` does not also throttle
  /// rendering `GET /two-factor`. Pair with an entry in
  /// [`GLOBAL_OVERRIDE_MUTATION_PREFIXES`] so the global layer still
  /// counts the `GET`s.
  pub fn mutations_only(mut self) -> Self {
    self.mutations_only = true;
    self
  }
}

impl topcoat::router::Layer for RateLimitLayer {
  fn path(&self) -> Option<&topcoat::router::Path> {
    self.path.as_deref()
  }

  fn handle<'a>(
    &'a self,
    cx: &'a topcoat::context::Cx,
    body: topcoat::router::Body,
    next: topcoat::router::Next<'a>,
  ) -> topcoat::router::LayerFuture<'a> {
    Box::pin(async move {
      let req_path = topcoat::router::request::uri(cx).path().to_owned();
      let mutation = is_mutation(topcoat::router::request::method(cx));
      if self.mutations_only && !mutation {
        return next.run(cx, body).await;
      }
      // True override: a scoped layer owns this path, let it enforce.
      let owned_elsewhere = self
        .skip_prefixes
        .iter()
        .any(|prefix| path_starts_with(&req_path, prefix))
        || (mutation
          && self
            .mutation_skip_prefixes
            .iter()
            .any(|prefix| path_starts_with(&req_path, prefix)));
      if owned_elsewhere {
        return next.run(cx, body).await;
      }
      let budget = limit_for(self.scope, self.per_minute);
      if !check_ip_rate(self.scope, &crate::valkey::client_ip(cx), budget) {
        return Err(rate_limit_error());
      }
      next.run(cx, body).await
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn form_limits_and_mutation_overrides_stay_in_sync() {
    let paths: Vec<&str> =
      FORM_RATE_LIMITS.iter().map(|(path, _, _)| *path).collect();
    assert_eq!(paths, GLOBAL_OVERRIDE_MUTATION_PREFIXES);
    // HTML twins share the API scopes (one bucket per abuse surface).
    for (path, scope) in [
      ("/forgot-password", SCOPE_PASSWORD_RESET),
      ("/reset-password", SCOPE_PASSWORD_RESET),
      ("/check-email/resend", SCOPE_VERIFY_EMAIL),
      ("/verify-email/resend", SCOPE_VERIFY_EMAIL),
      ("/two-factor", SCOPE_TOTP),
      ("/carrinho/cupom", SCOPE_COUPON),
    ] {
      assert!(
        FORM_RATE_LIMITS
          .iter()
          .any(|(p, s, _)| *p == path && *s == scope),
        "{path} must be limited under {scope}"
      );
    }
  }

  #[test]
  fn only_unsafe_methods_are_mutations() {
    assert!(!is_mutation(&http::Method::GET));
    assert!(!is_mutation(&http::Method::HEAD));
    assert!(!is_mutation(&http::Method::OPTIONS));
    assert!(is_mutation(&http::Method::POST));
    assert!(is_mutation(&http::Method::PUT));
    assert!(is_mutation(&http::Method::DELETE));
  }

  #[test]
  fn path_prefix_is_segment_aware() {
    assert!(path_starts_with("/pedido/123", "/pedido"));
    assert!(path_starts_with("/pedido", "/pedido"));
    assert!(!path_starts_with("/pedido2", "/pedido"));
    assert!(!path_starts_with("/pedido-status", "/pedido"));
    assert!(path_starts_with(
      "/api/auth/sign-in/email",
      "/api/auth/sign-in"
    ));
    assert!(!path_starts_with(
      "/api/auth/sign-upstream",
      "/api/auth/sign-up"
    ));
  }
}
