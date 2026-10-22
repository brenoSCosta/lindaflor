//! "Avise-me" subscribe endpoint: validation, dedupe and the per-IP budget.

use http::{Method, Request, StatusCode, header};
use sqlx::PgPool;
use test_support::pool_and_router;
use topcoat::router::{Body, Router};
use uuid::Uuid;

fn link_routes() {
  let _ = lindaflor::app::avise_me::subscribe;
}

async fn post(router: &Router, body: String) -> http::Response<Body> {
  let req = Request::builder()
    .method(Method::POST)
    .uri("/avise-me")
    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
    .body(Body::from(body))
    .expect("build request");
  router.handle(req).await
}

/// Sold-out variant (one warehouse row with zero units).
async fn seed_sold_out_variant(pool: &PgPool) -> Uuid {
  let product_id = Uuid::now_v7();
  let variant_id = Uuid::now_v7();
  let warehouse_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO products (id, name, slug, price_in_cents)
     VALUES ($1, 'Saída Brisa', $2, 8990)",
  )
  .bind(product_id)
  .bind(format!("saida-{}", product_id.simple()))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO product_variants (id, product_id, sku, size, color)
     VALUES ($1, $2, $3, 'p', 'Branco')",
  )
  .bind(variant_id)
  .bind(product_id)
  .bind(format!("sku-{variant_id}"))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO warehouses (id, code, name, is_default, active)
     VALUES ($1, $2, 'Principal', false, true)",
  )
  .bind(warehouse_id)
  .bind(format!("wh-{}", warehouse_id.simple()))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
     VALUES ($1, $2, $3, 0, 0)",
  )
  .bind(Uuid::now_v7())
  .bind(variant_id)
  .bind(warehouse_id)
  .execute(pool)
  .await
  .unwrap();
  variant_id
}

async fn alerts(pool: &PgPool, variant_id: Uuid) -> Vec<String> {
  sqlx::query_scalar(
    "SELECT email_norm FROM stock_alerts WHERE variant_id = $1 ORDER BY email_norm",
  )
  .bind(variant_id)
  .fetch_all(pool)
  .await
  .unwrap()
}

#[tokio::test]
async fn subscribe_validates_dedupes_and_is_rate_limited() {
  link_routes();
  let (pool, router) = pool_and_router().await;
  let variant = seed_sold_out_variant(&pool).await;

  // 1: invalid address → back to the product page, nothing stored.
  let response =
    post(&router, format!("variant_id={variant}&email=not-an-email")).await;
  assert_eq!(response.status(), StatusCode::SEE_OTHER);
  let location = response.headers()[header::LOCATION].to_str().unwrap();
  assert!(
    location.contains(&format!("variant={variant}")),
    "{location}"
  );
  assert!(alerts(&pool, variant).await.is_empty());

  // 2 + 3: stored once, normalized; the duplicate is a no-op.
  for email in ["Ana%40Example.com", "ana%40example.com"] {
    let response =
      post(&router, format!("variant_id={variant}&email={email}")).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
  }
  assert_eq!(alerts(&pool, variant).await, ["ana@example.com"]);

  // 4: honeypot filled → silently dropped.
  post(
    &router,
    format!("variant_id={variant}&email=bot%40example.com&website=x"),
  )
  .await;
  assert_eq!(alerts(&pool, variant).await, ["ana@example.com"]);

  // 5 is the last request within the per-minute budget; the 6th is refused.
  post(
    &router,
    format!("variant_id={variant}&email=bia%40example.com"),
  )
  .await;
  post(
    &router,
    format!("variant_id={variant}&email=cris%40example.com"),
  )
  .await;
  assert_eq!(
    alerts(&pool, variant).await,
    ["ana@example.com", "bia@example.com"]
  );
}
