//! Security review task group C: HTML-form rate limits and the coupon
//! apply budget. (`GET /checkout` is a module-routed page, which the test
//! router does not mount, so the read-only cart lookup is not covered
//! here.)

use http::{Method, Request, StatusCode, header};
use test_support::pool_and_router;
use topcoat::router::{Body, Router};

/// Tests in this binary rebuild the router (which resets the generous test
/// `RATE_LIMIT_*` env) and tighten budgets through process-global env vars
/// before the first request creates a governor quota, so they run one at a
/// time.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn link_routes() {
  let _ = lindaflor::app::forgot_password::page;
  let _ = lindaflor::app::forgot_password::forgot_post;
  let _ = lindaflor::app::store::cart::set_cart_coupon_proc;
}

async fn send(
  router: &Router,
  method: Method,
  path: &str,
  form: Option<&str>,
) -> http::Response<Body> {
  let builder = Request::builder().method(method).uri(path);
  let req = match form {
    Some(body) => builder
      .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
      .body(Body::from(body.to_owned())),
    None => builder.body(Body::empty()),
  }
  .expect("build request");
  router.handle(req).await
}

#[tokio::test]
async fn forgot_password_post_is_scoped_but_page_is_not() {
  let _serial = SERIAL.lock().await;
  link_routes();
  let (_pool, router) = pool_and_router().await;
  // Tighten after the router build (which installs 10000) and before the
  // first request to this scope creates its quota.
  unsafe { std::env::set_var("RATE_LIMIT_AUTH_PASSWORD_RESET_PER_MIN", "2") };

  // GETs render the page and never touch the strict POST budget.
  for _ in 0..4 {
    let response = send(&router, Method::GET, "/forgot-password", None).await;
    // (The test router can't fully render storefront pages, so only the
    // absence of a 429 is asserted.)
    assert_ne!(response.status(), StatusCode::TOO_MANY_REQUESTS);
  }

  let body = "email=nobody%40example.com";
  for i in 0..2 {
    let response =
      send(&router, Method::POST, "/forgot-password", Some(body)).await;
    assert_eq!(
      response.status(),
      StatusCode::SEE_OTHER,
      "POST {} within budget",
      i + 1
    );
  }
  let response =
    send(&router, Method::POST, "/forgot-password", Some(body)).await;
  assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

  // Shared bucket with the JSON twin: switching to the API doesn't reset it.
  let response = send(
    &router,
    Method::POST,
    "/api/auth/request-password-reset",
    Some(body),
  )
  .await;
  assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn coupon_apply_has_a_scoped_budget() {
  let _serial = SERIAL.lock().await;
  link_routes();
  let (_pool, router) = pool_and_router().await;
  unsafe { std::env::set_var("RATE_LIMIT_COUPON_APPLY_PER_MIN", "1") };

  let first = send(&router, Method::POST, "/carrinho/cupom", Some("x")).await;
  assert_ne!(first.status(), StatusCode::TOO_MANY_REQUESTS);
  let second = send(&router, Method::POST, "/carrinho/cupom", Some("x")).await;
  assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}
