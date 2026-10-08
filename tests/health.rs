use http::StatusCode;
use test_support::{json_get, pool_and_router};

#[tokio::test]
async fn health_check_returns_ok() {
  let (_pool, router) = pool_and_router().await;
  let response = json_get(&router, "/api/health", None).await;
  assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
  assert_eq!(response.json["status"], "ok");
}
