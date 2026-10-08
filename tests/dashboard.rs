use http::{HeaderMap, Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use test_support::{pool_and_router, sign_up, unique_email};
use topcoat::router::{Body, Router};
use uuid::Uuid;

async fn get_page(
  router: &Router,
  path: &str,
  cookie: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
  let mut builder = Request::builder().method(Method::GET).uri(path);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }
  let req = builder.body(Body::empty()).expect("build request");
  let response = router.handle(req).await;
  let status = response.status();
  let headers = response.headers().clone();
  let bytes = response
    .into_body()
    .collect()
    .await
    .expect("collect body")
    .to_bytes();
  let body = String::from_utf8_lossy(&bytes).into_owned();
  (status, headers, body)
}

async fn insert_order(
  pool: &sqlx::PgPool,
  user_id: Uuid,
  total_cents: i32,
) -> Uuid {
  let id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO orders (id, user_id, status, total_cents, access_token)
     VALUES ($1, $2, 'paid', $3, $4)",
  )
  .bind(id)
  .bind(user_id)
  .bind(total_cents)
  .bind(Uuid::now_v7())
  .execute(pool)
  .await
  .expect("insert order");
  id
}

fn user_id(json: &serde_json::Value) -> Uuid {
  Uuid::parse_str(json["user"]["id"].as_str().unwrap()).unwrap()
}

#[ignore = "test router does not mount module pages"]
#[tokio::test]
async fn anonymous_dashboard_redirects_to_login() {
  let (pool, router) = pool_and_router().await;

  // Seed an order so a leak would be observable in the response body.
  let signed_up = sign_up(
    &router,
    "Dashboard User",
    &unique_email("dash"),
    "password123",
  )
  .await;
  assert_eq!(signed_up.status, StatusCode::OK);
  let order_id = insert_order(&pool, user_id(&signed_up.json), 424242).await;

  let (status, headers, body) = get_page(&router, "/dashboard", None).await;
  assert!(
    status.is_redirection(),
    "anonymous GET /dashboard must redirect, got {status}"
  );
  let location = headers
    .get(header::LOCATION)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("");
  assert!(
    location.contains("login"),
    "anonymous /dashboard must redirect to login, got Location: {location:?}"
  );
  assert!(
    !body.contains(&order_id.to_string()),
    "anonymous response must never render order rows"
  );
  assert!(
    !body.contains("Pedidos recentes"),
    "anonymous response must never render the recent-orders table"
  );
}

#[ignore = "test router does not mount module pages"]
#[tokio::test]
async fn dashboard_scopes_orders_to_current_user() {
  let (pool, router) = pool_and_router().await;

  let a =
    sign_up(&router, "User A", &unique_email("dash-a"), "password123").await;
  assert_eq!(a.status, StatusCode::OK);
  let b =
    sign_up(&router, "User B", &unique_email("dash-b"), "password123").await;
  assert_eq!(b.status, StatusCode::OK);
  let cookie_a = a.next_cookie.clone().expect("cookie a");

  let order_a = insert_order(&pool, user_id(&a.json), 424242).await;
  let order_b = insert_order(&pool, user_id(&b.json), 777777).await;

  let (status, _, body) =
    get_page(&router, "/dashboard", Some(&cookie_a)).await;
  assert_eq!(
    status,
    StatusCode::OK,
    "authenticated /dashboard must render"
  );

  let short_a: String = order_a.to_string().chars().take(8).collect();
  assert!(
    body.contains(&short_a),
    "dashboard must list the current user's own order"
  );
  assert!(
    body.contains("R$ 4242"),
    "dashboard must render the current user's order total"
  );
  assert!(
    !body.contains(&order_b.to_string()),
    "dashboard must not leak another user's order id"
  );
  assert!(
    !body.contains("7777"),
    "dashboard must not render another user's order total"
  );
}

#[tokio::test]
async fn load_recent_orders_is_scoped_to_owner() {
  let (pool, router) = pool_and_router().await;

  let a =
    sign_up(&router, "User A", &unique_email("dash-a"), "password123").await;
  assert_eq!(a.status, StatusCode::OK);
  let b =
    sign_up(&router, "User B", &unique_email("dash-b"), "password123").await;
  assert_eq!(b.status, StatusCode::OK);

  let order_a = insert_order(&pool, user_id(&a.json), 424242).await;
  let order_b = insert_order(&pool, user_id(&b.json), 777777).await;

  let rows_a =
    lindaflor::app::dashboard::load_recent_orders(&pool, user_id(&a.json))
      .await
      .expect("load a");
  let ids_a: Vec<_> = rows_a.iter().map(|r| r.id).collect();
  assert!(ids_a.contains(&order_a));
  assert!(!ids_a.contains(&order_b));
  assert!(rows_a.iter().all(|r| r.total_cents != 777777));

  let rows_b =
    lindaflor::app::dashboard::load_recent_orders(&pool, user_id(&b.json))
      .await
      .expect("load b");
  let ids_b: Vec<_> = rows_b.iter().map(|r| r.id).collect();
  assert!(ids_b.contains(&order_b));
  assert!(!ids_b.contains(&order_a));
}
