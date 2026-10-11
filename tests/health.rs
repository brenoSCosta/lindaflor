use http::StatusCode;
use test_support::{json_get, pool_and_router};

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
