use http::{Method, Request, StatusCode, header};
use test_support::{body_bytes, json_get, pool_and_router};
use topcoat::router::Body;

#[tokio::test]
async fn health_check_returns_ok() {
  let (_pool, router) = pool_and_router().await;
  let response = json_get(&router, "/api/health", None).await;
  assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
  assert_eq!(response.json["status"], "ok");
}

#[tokio::test]
async fn readyz_reports_ready_when_backends_healthy() {
  let (_pool, router) = pool_and_router().await;
  let response = json_get(&router, "/api/readyz", None).await;
  assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
  assert_eq!(response.json["status"], "ok");
  assert_eq!(response.json["database"], "ok");
}

#[tokio::test]
async fn readyz_returns_503_when_database_degraded() {
  let (pool, router) = pool_and_router().await;
  // Draining the pool makes `SELECT 1` fail, simulating a DB outage.
  pool.close().await;
  let response = json_get(&router, "/api/readyz", None).await;
  assert_eq!(
    response.status,
    StatusCode::SERVICE_UNAVAILABLE,
    "{:?}",
    response.json
  );
  assert_eq!(response.json["status"], "degraded");
  assert_eq!(response.json["database"], "degraded");
}

#[tokio::test]
async fn default_body_limit_rejects_over_1mb() {
  let (_pool, router) = pool_and_router().await;
  let req = Request::builder()
    .method(Method::POST)
    .uri("/api/auth/sign-in/email")
    .header(header::CONTENT_TYPE, "application/json")
    .body(Body::from(vec![b'x'; 1024 * 1024 + 1]))
    .expect("request");
  let response = router.handle(req).await;
  assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
  let body = String::from_utf8_lossy(&body_bytes(response).await).into_owned();
  assert!(!body.to_lowercase().contains("sqlx"), "{body}");
}
