//! Payment integration tests against the mock Mercado Pago server
//! ([`super::test_mock`]) and the embedded test Postgres.

use std::time::Duration;

use http::{Method, Request, StatusCode};
use sqlx::PgPool;
use topcoat::router::{Body, Router};
use uuid::Uuid;

use super::circuit::CircuitBreaker;
use super::service::{
  self, AdminAction, MERCADO_PAGO_ACTOR, ServiceError, SyncOutcome,
};
use super::test_mock::{MockMp, SECRET};
use super::{PaymentError, PaymentProvider, signature};
use crate::app::store::inventory::{ReserveLine, reserve_for_order};

// ---------------------------------------------------------------------------
// DB helpers
// ---------------------------------------------------------------------------

/// A pending Mercado Pago order holding 1 unit of a variant with `stock`
/// units on hand. Returns `(order_id, variant_id)`.
async fn seed_mp_order(
  pool: &PgPool,
  stock: i32,
  total_cents: i64,
) -> (Uuid, Uuid) {
  let product_id = Uuid::now_v7();
  let variant_id = Uuid::now_v7();
  let warehouse_id = Uuid::now_v7();
  let order_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'Biquíni', $2, $3)",
  )
  .bind(product_id)
  .bind(format!("mp-{product_id}"))
  .bind(total_cents)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO product_variants (id, product_id, sku, size, color)
     VALUES ($1, $2, $3, 'm', 'azul')",
  )
  .bind(variant_id)
  .bind(product_id)
  .bind(format!("sku-{variant_id}"))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO warehouses (id, code, name, is_default, active)
     VALUES ($1, $2, 'W', false, true)",
  )
  .bind(warehouse_id)
  .bind(format!("wh-{}", warehouse_id.simple()))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
     VALUES ($1, $2, $3, $4, 0)",
  )
  .bind(Uuid::now_v7())
  .bind(variant_id)
  .bind(warehouse_id)
  .bind(stock)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO orders (id, guest_email, status, access_token, subtotal_cents, total_cents, payment_provider)
     VALUES ($1, 'cliente@example.com', 'pending_payment', $2, $3, $3, 'mercadopago')",
  )
  .bind(order_id)
  .bind(Uuid::new_v4())
  .bind(total_cents)
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
     VALUES ($1, $2, $3, 'Biquíni', 'M / azul', 1, $4)",
  )
  .bind(Uuid::now_v7())
  .bind(order_id)
  .bind(variant_id)
  .bind(total_cents)
  .execute(pool)
  .await
  .unwrap();
  let mut tx = pool.begin().await.unwrap();
  reserve_for_order(
    &mut tx,
    order_id,
    &[ReserveLine {
      variant_id,
      quantity: 1,
    }],
  )
  .await
  .unwrap();
  tx.commit().await.unwrap();
  (order_id, variant_id)
}

/// Seed + create the MP charge. Returns `(order_id, variant_id, payment_id)`.
async fn seed_charged_order(
  pool: &PgPool,
  provider: &PaymentProvider,
  stock: i32,
) -> (Uuid, Uuid, i64) {
  let (order_id, variant_id) = seed_mp_order(pool, stock, 12_345).await;
  let view = service::ensure_charge(pool, provider, order_id)
    .await
    .expect("charge")
    .expect("mp order");
  (order_id, variant_id, view.payment_id)
}

async fn order_status(pool: &PgPool, order_id: Uuid) -> String {
  sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
    .bind(order_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn stock(pool: &PgPool, variant_id: Uuid) -> (i32, i32) {
  sqlx::query_as(
    "SELECT quantity, reserved FROM inventory WHERE variant_id = $1",
  )
  .bind(variant_id)
  .fetch_one(pool)
  .await
  .unwrap()
}

async fn expire_hold(pool: &PgPool, order_id: Uuid, ago: &str) {
  sqlx::query(&format!(
    "UPDATE orders SET reservation_expires_at = now() - interval '{ago}' WHERE id = $1"
  ))
  .bind(order_id)
  .execute(pool)
  .await
  .unwrap();
}

async fn mp_audit_count(pool: &PgPool, order_id: Uuid) -> i64 {
  sqlx::query_scalar(
    "SELECT count(*) FROM audit_events WHERE entity_id = $1 AND actor_name = 'Mercado Pago'",
  )
  .bind(order_id)
  .fetch_one(pool)
  .await
  .unwrap()
}

// ---------------------------------------------------------------------------
// Webhook router helpers
// ---------------------------------------------------------------------------

fn webhook_router(pool: PgPool, provider: PaymentProvider) -> Router {
  Router::builder()
    .route(super::webhook::mercadopago_webhook)
    .app_context(pool)
    .app_context(provider)
    // Same exemption as `src/app.rs`.
    .origin_policy(
      topcoat::router::OriginPolicy::new()
        .exempt_paths([super::webhook::WEBHOOK_PATH]),
    )
    .build()
}

fn now() -> i64 {
  time::OffsetDateTime::now_utc().unix_timestamp()
}

async fn post_webhook(
  router: &Router,
  data_id: &str,
  secret: &str,
  ts: i64,
  signature_header: Option<String>,
) -> StatusCode {
  let request_id = Uuid::new_v4().to_string();
  let ts = ts.to_string();
  let header = signature_header.unwrap_or_else(|| {
    let v1 = signature::sign(
      secret,
      &signature::manifest(Some(data_id), Some(&request_id), &ts),
    );
    format!("ts={ts},v1={v1}")
  });
  let body = serde_json::json!({
    "action": "payment.updated",
    "type": "payment",
    "data": { "id": data_id },
  });
  let request = Request::builder()
    .method(Method::POST)
    .uri(format!(
      "/api/webhooks/mercadopago?data.id={data_id}&type=payment"
    ))
    // A server POST from another origin: OriginPolicy must let it through.
    .header("origin", "https://www.mercadopago.com.br")
    .header("content-type", "application/json")
    .header("x-request-id", request_id)
    .header("x-signature", header)
    .body(Body::from(body.to_string().into_bytes()))
    .unwrap();
  router.handle(request).await.status()
}

// ---------------------------------------------------------------------------
// Client: create / retry / circuit breaker
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_charge_uses_order_amount_and_idempotency_key() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, _, payment_id) = seed_charged_order(&pool, &provider, 3).await;

  let posts: Vec<_> = mock
    .requests()
    .into_iter()
    .filter(|r| r.method == "POST")
    .collect();
  assert_eq!(posts.len(), 1);
  let create = &posts[0];
  assert_eq!(
    create.idempotency_key.as_deref(),
    Some(order_id.to_string().as_str())
  );
  assert_eq!(create.body["transaction_amount"], serde_json::json!(123.45));
  assert_eq!(create.body["payment_method_id"], "pix");
  assert_eq!(create.body["payer"]["email"], "cliente@example.com");
  assert_eq!(create.body["external_reference"], order_id.to_string());
  assert!(
    create.body["date_of_expiration"]
      .as_str()
      .is_some_and(|d| d.ends_with("-03:00")),
    "{:?}",
    create.body
  );

  let stored: (Option<i64>, Option<String>, Option<String>) = sqlx::query_as(
    "SELECT mp_payment_id, mp_qr_code, mp_status FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(stored.0, Some(payment_id));
  assert_eq!(stored.1, Some(format!("00020126MOCKPIX{payment_id}")));
  assert_eq!(stored.2.as_deref(), Some("pending"));

  // The order page reuses the stored charge: no second POST.
  match service::order_payment(&pool, &provider, order_id)
    .await
    .unwrap()
  {
    service::OrderPayment::MercadoPago(Some(view)) => {
      assert_eq!(view.payment_id, payment_id);
      assert!(view.qr_code_base64.is_some());
      assert!(view.expires_label.is_some());
    }
    other => panic!("unexpected {other:?}"),
  }
  assert_eq!(mock.count("POST", "/v1/payments"), 1);
}

#[tokio::test]
async fn create_charge_retries_5xx_with_same_idempotency_key() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, _) = seed_mp_order(&pool, 3, 5_000).await;
  mock.force(&[500, 503]);
  service::ensure_charge(&pool, &provider, order_id)
    .await
    .expect("third attempt succeeds")
    .expect("charge");
  let posts = mock.requests();
  assert_eq!(posts.len(), 3);
  assert!(posts.iter().all(
    |r| r.idempotency_key.as_deref() == Some(order_id.to_string().as_str())
  ));
}

#[tokio::test]
async fn create_charge_does_not_retry_4xx() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, _) = seed_mp_order(&pool, 3, 5_000).await;
  mock.force(&[400]);
  let error = service::ensure_charge(&pool, &provider, order_id)
    .await
    .expect_err("4xx is final");
  assert!(
    matches!(
      error,
      ServiceError::Payment(PaymentError::Rejected { status: 400, .. })
    ),
    "{error:?}"
  );
  assert_eq!(mock.requests().len(), 1);
  let stored: Option<i64> =
    sqlx::query_scalar("SELECT mp_payment_id FROM orders WHERE id = $1")
      .bind(order_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(stored, None);
  // A 4xx says nothing about MP's health: the breaker stays closed.
  assert!(provider.accepting_charges());
}

#[tokio::test]
async fn exhausted_retries_open_the_circuit_and_fail_fast() {
  let mock = MockMp::start().await;
  let provider =
    mock.provider_with(CircuitBreaker::new(1, Duration::from_secs(60)));
  mock.force(&[500, 502, 504]);
  let error = provider
    .fetch_status(1)
    .await
    .expect_err("all attempts fail");
  assert!(matches!(error, PaymentError::Transient(_)), "{error:?}");
  assert_eq!(mock.requests().len(), 3, "max 3 attempts");

  assert!(!provider.accepting_charges());
  let error = provider.fetch_status(1).await.expect_err("circuit open");
  assert!(matches!(error, PaymentError::Unavailable), "{error:?}");
  assert_eq!(
    error.to_string(),
    "Pagamento temporariamente indisponível. Tente novamente em instantes."
  );
  assert_eq!(mock.requests().len(), 3, "no HTTP call while open");
}

// ---------------------------------------------------------------------------
// Webhook
// ---------------------------------------------------------------------------

#[tokio::test]
async fn webhook_approved_marks_paid_and_converts_reservation() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, variant_id, payment_id) =
    seed_charged_order(&pool, &provider, 3).await;
  assert_eq!(stock(&pool, variant_id).await, (3, 1));
  mock.set_status(payment_id, "approved");

  let router = webhook_router(pool.clone(), provider);
  let status =
    post_webhook(&router, &payment_id.to_string(), SECRET, now(), None).await;
  assert_eq!(status, StatusCode::OK);
  assert_eq!(order_status(&pool, order_id).await, "paid");
  // Reservation converted into a sale: on-hand down by one, hold released.
  assert_eq!(stock(&pool, variant_id).await, (2, 0));
  let (mp_status, paid): (Option<String>, bool) = sqlx::query_as(
    "SELECT mp_status, paid_at IS NOT NULL FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(mp_status.as_deref(), Some("approved"));
  assert!(paid);
  assert_eq!(mp_audit_count(&pool, order_id).await, 1);
}

#[tokio::test]
async fn webhook_rejects_bad_and_stale_signatures() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, _, payment_id) = seed_charged_order(&pool, &provider, 3).await;
  mock.set_status(payment_id, "approved");
  let router = webhook_router(pool.clone(), provider);
  let id = payment_id.to_string();

  // Signed with the wrong secret.
  assert_eq!(
    post_webhook(&router, &id, "not-the-secret", now(), None).await,
    StatusCode::UNAUTHORIZED
  );
  // Valid HMAC but outside the replay window.
  assert_eq!(
    post_webhook(&router, &id, SECRET, now() - 3600, None).await,
    StatusCode::UNAUTHORIZED
  );
  // Garbage / signature for another payment id.
  assert_eq!(
    post_webhook(&router, &id, SECRET, now(), Some("ts=1,v1=00".to_string()))
      .await,
    StatusCode::UNAUTHORIZED
  );
  let ts = now().to_string();
  let other =
    signature::sign(SECRET, &signature::manifest(Some("1"), None, &ts));
  assert_eq!(
    post_webhook(
      &router,
      &id,
      SECRET,
      now(),
      Some(format!("ts={ts},v1={other}"))
    )
    .await,
    StatusCode::UNAUTHORIZED
  );

  assert_eq!(order_status(&pool, order_id).await, "pending_payment");
  assert_eq!(mock.count("GET", "/v1/payments"), 0, "never fetched");
}

#[tokio::test]
async fn webhook_redelivery_applies_once() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, variant_id, payment_id) =
    seed_charged_order(&pool, &provider, 3).await;
  mock.set_status(payment_id, "approved");
  let router = webhook_router(pool.clone(), provider);
  let id = payment_id.to_string();

  // Concurrent duplicate delivery, then a late third one.
  let (a, b) = tokio::join!(
    post_webhook(&router, &id, SECRET, now(), None),
    post_webhook(&router, &id, SECRET, now(), None),
  );
  assert_eq!((a, b), (StatusCode::OK, StatusCode::OK));
  assert_eq!(
    post_webhook(&router, &id, SECRET, now(), None).await,
    StatusCode::OK
  );

  assert_eq!(order_status(&pool, order_id).await, "paid");
  assert_eq!(stock(&pool, variant_id).await, (2, 0), "stock moved once");
  assert_eq!(mp_audit_count(&pool, order_id).await, 1, "one transition");
}

#[tokio::test]
async fn webhook_rejected_payment_cancels_and_releases_stock() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, variant_id, payment_id) =
    seed_charged_order(&pool, &provider, 3).await;
  mock.set_status(payment_id, "cancelled");
  let router = webhook_router(pool.clone(), provider);
  assert_eq!(
    post_webhook(&router, &payment_id.to_string(), SECRET, now(), None).await,
    StatusCode::OK
  );
  assert_eq!(order_status(&pool, order_id).await, "cancelled");
  assert_eq!(stock(&pool, variant_id).await, (3, 0));
}

#[tokio::test]
async fn approval_with_wrong_amount_is_not_applied() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, _, payment_id) = seed_charged_order(&pool, &provider, 3).await;
  mock.set_status(payment_id, "approved");
  mock.set_field(payment_id, "transaction_amount", serde_json::json!(1.0));
  let outcome =
    service::sync_payment(&pool, &provider, payment_id, MERCADO_PAGO_ACTOR)
      .await
      .unwrap();
  assert_eq!(outcome, SyncOutcome::Mismatch { order_id });
  assert_eq!(order_status(&pool, order_id).await, "pending_payment");
}

// ---------------------------------------------------------------------------
// Expiry sweep + late approval
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sweep_marks_paid_when_mp_approved_before_expiry_cancel() {
  let pool = crate::test_support::isolated_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, variant_id, payment_id) =
    seed_charged_order(&pool, &provider, 3).await;
  expire_hold(&pool, order_id, "1 minute").await;
  // Paid, but the webhook never arrived.
  mock.set_status(payment_id, "approved");
  service::expire_pending_reservations(&pool, &provider)
    .await
    .unwrap();
  assert_eq!(order_status(&pool, order_id).await, "paid");
  assert_eq!(stock(&pool, variant_id).await, (2, 0));
}

#[tokio::test]
async fn late_approval_after_expiry_reopens_and_pays() {
  let pool = crate::test_support::isolated_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, variant_id, payment_id) =
    seed_charged_order(&pool, &provider, 3).await;
  expire_hold(&pool, order_id, "1 minute").await;

  // MP still says pending at sweep time: the hold is released.
  service::expire_pending_reservations(&pool, &provider)
    .await
    .unwrap();
  assert_eq!(order_status(&pool, order_id).await, "cancelled");
  assert_eq!(stock(&pool, variant_id).await, (3, 0));

  // The buyer pays right after: the webhook re-reserves and marks paid.
  mock.set_status(payment_id, "approved");
  let router = webhook_router(pool.clone(), provider);
  assert_eq!(
    post_webhook(&router, &payment_id.to_string(), SECRET, now(), None).await,
    StatusCode::OK
  );
  assert_eq!(order_status(&pool, order_id).await, "paid");
  assert_eq!(stock(&pool, variant_id).await, (2, 0));
}

#[tokio::test]
async fn late_approval_without_stock_needs_attention() {
  let pool = crate::test_support::isolated_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, variant_id, payment_id) =
    seed_charged_order(&pool, &provider, 1).await;
  expire_hold(&pool, order_id, "1 minute").await;
  service::expire_pending_reservations(&pool, &provider)
    .await
    .unwrap();
  // Someone else bought the last unit meanwhile.
  sqlx::query("UPDATE inventory SET quantity = 0 WHERE variant_id = $1")
    .bind(variant_id)
    .execute(&pool)
    .await
    .unwrap();
  mock.set_status(payment_id, "approved");
  let outcome =
    service::sync_payment(&pool, &provider, payment_id, MERCADO_PAGO_ACTOR)
      .await
      .unwrap();
  assert_eq!(outcome, SyncOutcome::NeedsAttention { order_id });
  assert_eq!(order_status(&pool, order_id).await, "cancelled");
  let mp_status: Option<String> =
    sqlx::query_scalar("SELECT mp_status FROM orders WHERE id = $1")
      .bind(order_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert_eq!(mp_status.as_deref(), Some("approved"));
  let info = service::admin_payment_info(&pool, order_id)
    .await
    .unwrap()
    .unwrap();
  assert!(info.refundable("cancelled"), "admin can refund it");
}

#[tokio::test]
async fn sweep_defers_cancel_while_mp_is_unreachable() {
  let pool = crate::test_support::isolated_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, _, _) = seed_charged_order(&pool, &provider, 3).await;
  expire_hold(&pool, order_id, "1 minute").await;
  mock.force(&[500, 500, 500]);
  service::expire_pending_reservations(&pool, &provider)
    .await
    .unwrap();
  assert_eq!(order_status(&pool, order_id).await, "pending_payment");

  // Past the grace window the hold is released regardless.
  expire_hold(&pool, order_id, "2 hours").await;
  mock.force(&[500, 500, 500]);
  service::expire_pending_reservations(&pool, &provider)
    .await
    .unwrap();
  assert_eq!(order_status(&pool, order_id).await, "cancelled");
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_refund_calls_mp_then_refunds_order() {
  let pool = crate::test_support::fresh_pool().await;
  let mock = MockMp::start().await;
  let provider = mock.provider();
  let (order_id, variant_id, payment_id) =
    seed_charged_order(&pool, &provider, 3).await;
  mock.set_status(payment_id, "approved");
  let actor = service::Actor {
    id: None,
    name: "Admin Teste",
  };
  let message = service::run_admin_action(
    &pool,
    &provider,
    order_id,
    AdminAction::Sync,
    actor,
  )
  .await
  .expect("sync");
  assert!(message.contains("sincronizado"), "{message}");
  assert_eq!(order_status(&pool, order_id).await, "paid");

  service::run_admin_action(
    &pool,
    &provider,
    order_id,
    AdminAction::Refund,
    actor,
  )
  .await
  .expect("refund");
  assert_eq!(
    mock.count("POST", &format!("/v1/payments/{payment_id}/refunds")),
    1
  );
  assert_eq!(order_status(&pool, order_id).await, "refunded");
  // Paid refund restocks by default.
  assert_eq!(stock(&pool, variant_id).await, (3, 0));
  let audited: i64 = sqlx::query_scalar(
    "SELECT count(*) FROM audit_events
     WHERE entity_id = $1 AND metadata->>'action' = 'mp_refund' AND actor_name = 'Admin Teste'",
  )
  .bind(order_id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(audited, 1);

  // A refund webhook arriving afterwards is a no-op.
  let outcome =
    service::sync_payment(&pool, &provider, payment_id, MERCADO_PAGO_ACTOR)
      .await
      .unwrap();
  assert_eq!(outcome, SyncOutcome::Unchanged { order_id });
}

#[tokio::test]
async fn static_pix_provider_has_no_webhook() {
  let pool = crate::test_support::fresh_pool().await;
  let router = webhook_router(pool, PaymentProvider::StaticPix);
  assert_eq!(
    post_webhook(&router, "1", SECRET, now(), None).await,
    StatusCode::NOT_FOUND
  );
}
