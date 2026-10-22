//! Back-in-stock alerts ("Avise-me", `stock_alerts`, migration 0029).
//!
//! Shoppers subscribe an address to a sold-out variant (see
//! `app::avise_me`). Delivery is a sweeper pass ([`notify_restocked`], run
//! from the background loop in `src/main.rs`), not a hook in each stock
//! write: stock comes back through admin adjustments, the product-edit
//! inventory sync, cancel/refund restocks *and* expired holds released by
//! the reservation sweep, and a sweeper that just asks "pending alert +
//! variant sellable now?" covers every one of them (including future
//! paths) without touching the inventory lock order. The cost is latency of
//! one sweep tick (`RESERVATION_SWEEP_SECS`, default 60s).
//!
//! Each alert mails once: the pass claims rows with `FOR UPDATE SKIP
//! LOCKED`, enqueues with a per-alert dedupe key and sets `notified_at` in
//! the same transaction. A variant that sells out again never re-sends; the
//! shopper subscribes again if they still want it.

use sqlx::PgPool;
use uuid::Uuid;

use crate::app::store::inventory::LIVE_AVAILABLE_SQL;
use crate::app::store::queries::size_label;
use crate::mail::outbox::{enqueue, kind};
use crate::mail::{absolute_url, encode_query_value, templates};

/// Path of the cancel page (GET confirm button, POST cancels).
pub const CANCEL_PATH: &str = "/avise-me/cancelar";

/// HMAC domain of the cancel link (see `newsletter::signed_email_token`).
pub const CANCEL_DOMAIN: &str = "stock-alert-cancel:v1:";

/// Pending alerts one address may hold: caps how much mail a stranger can
/// sign someone else up for.
pub const MAX_PENDING_PER_EMAIL: i64 = 20;

/// Alerts claimed per sweep pass.
const SWEEP_BATCH: i64 = 200;

/// Result of [`subscribe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscribeOutcome {
  Subscribed,
  /// A pending alert for this variant and address already exists.
  AlreadySubscribed,
  /// The variant is sellable right now: nothing to wait for.
  InStock,
  /// The address hit [`MAX_PENDING_PER_EMAIL`].
  TooMany,
  /// Unknown variant or inactive product.
  UnknownVariant,
}

/// Active product + live available units of a variant, `None` if unknown.
async fn variant_stock(
  pool: &PgPool,
  variant_id: Uuid,
) -> Result<Option<(bool, i64)>, sqlx::Error> {
  sqlx::query_as::<_, (bool, i64)>(&format!(
    "SELECT p.active,
            COALESCE((
              SELECT SUM({LIVE_AVAILABLE_SQL})
              FROM inventory i
              INNER JOIN warehouses w ON w.id = i.warehouse_id
              WHERE i.variant_id = pv.id AND w.active = true
            ), 0)::bigint
     FROM product_variants pv
     INNER JOIN products p ON p.id = pv.product_id
     WHERE pv.id = $1"
  ))
  .bind(variant_id)
  .fetch_optional(pool)
  .await
}

/// Product slug of a variant (for redirects back to its page).
pub async fn variant_slug(
  pool: &PgPool,
  variant_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
  sqlx::query_scalar(
    "SELECT p.slug FROM product_variants pv
     INNER JOIN products p ON p.id = pv.product_id
     WHERE pv.id = $1",
  )
  .bind(variant_id)
  .fetch_optional(pool)
  .await
}

/// Register an alert. `email_norm` must already be validated and
/// normalized (`auth::validation::validate_email`).
pub async fn subscribe(
  pool: &PgPool,
  variant_id: Uuid,
  email_norm: &str,
  user_id: Option<Uuid>,
) -> Result<SubscribeOutcome, sqlx::Error> {
  match variant_stock(pool, variant_id).await? {
    None | Some((false, _)) => return Ok(SubscribeOutcome::UnknownVariant),
    Some((true, available)) if available > 0 => {
      return Ok(SubscribeOutcome::InStock);
    }
    Some(_) => {}
  }
  let pending: i64 = sqlx::query_scalar(
    "SELECT count(*) FROM stock_alerts
     WHERE email_norm = $1 AND notified_at IS NULL",
  )
  .bind(email_norm)
  .fetch_one(pool)
  .await?;
  if pending >= MAX_PENDING_PER_EMAIL {
    return Ok(SubscribeOutcome::TooMany);
  }
  let inserted = sqlx::query(
    "INSERT INTO stock_alerts (id, variant_id, email_norm, user_id)
     VALUES ($1, $2, $3, $4)
     ON CONFLICT (variant_id, email_norm) WHERE notified_at IS NULL
     DO NOTHING",
  )
  .bind(Uuid::now_v7())
  .bind(variant_id)
  .bind(email_norm)
  .bind(user_id)
  .execute(pool)
  .await?
  .rows_affected();
  Ok(if inserted > 0 {
    SubscribeOutcome::Subscribed
  } else {
    SubscribeOutcome::AlreadySubscribed
  })
}

/// Drop every pending alert of `email` (the signed cancel link). Returns
/// how many were removed.
pub async fn cancel_all(
  pool: &PgPool,
  email: &str,
) -> Result<u64, sqlx::Error> {
  Ok(
    sqlx::query(
      "DELETE FROM stock_alerts WHERE email_norm = $1 AND notified_at IS NULL",
    )
    .bind(email.trim().to_lowercase())
    .execute(pool)
    .await?
    .rows_affected(),
  )
}

/// Absolute signed cancel link for `email`.
pub fn cancel_url(email: &str) -> String {
  let email = email.trim().to_lowercase();
  let token = crate::app::newsletter::signed_email_token(CANCEL_DOMAIN, &email);
  absolute_url(&format!(
    "{CANCEL_PATH}?email={}&token={token}",
    encode_query_value(&email)
  ))
}

pub fn verify_cancel_token(email: &str, token: &str) -> bool {
  crate::app::newsletter::verify_signed_email_token(CANCEL_DOMAIN, email, token)
}

#[derive(sqlx::FromRow)]
struct DueAlert {
  id: Uuid,
  email_norm: String,
  variant_id: Uuid,
  product_name: String,
  product_slug: String,
  size: String,
  color: String,
}

/// Sweeper pass: mail every pending alert whose variant is sellable again
/// (active product, live available > 0) and close it. Returns the number
/// of alerts mailed.
pub async fn notify_restocked(pool: &PgPool) -> Result<u64, sqlx::Error> {
  let mut tx = pool.begin().await?;
  let due: Vec<DueAlert> = sqlx::query_as(&format!(
    "SELECT a.id, a.email_norm, pv.id AS variant_id,
            p.name AS product_name, p.slug AS product_slug,
            pv.size::text AS size, pv.color
     FROM stock_alerts a
     INNER JOIN product_variants pv ON pv.id = a.variant_id
     INNER JOIN products p ON p.id = pv.product_id
     WHERE a.notified_at IS NULL
       AND p.active = true
       AND COALESCE((
         SELECT SUM({LIVE_AVAILABLE_SQL})
         FROM inventory i
         INNER JOIN warehouses w ON w.id = i.warehouse_id
         WHERE i.variant_id = pv.id AND w.active = true
       ), 0) > 0
     ORDER BY a.created_at
     LIMIT $1
     FOR UPDATE OF a SKIP LOCKED"
  ))
  .bind(SWEEP_BATCH)
  .fetch_all(&mut *tx)
  .await?;
  if due.is_empty() {
    tx.rollback().await?;
    return Ok(0);
  }

  let mut ids = Vec::with_capacity(due.len());
  for alert in &due {
    let product_url = absolute_url(&format!(
      "/produtos/{}?variant={}",
      encode_query_value(&alert.product_slug),
      alert.variant_id
    ));
    let label = format!("{} · {}", size_label(&alert.size), alert.color);
    let email = templates::back_in_stock(
      &alert.product_name,
      &label,
      &product_url,
      &cancel_url(&alert.email_norm),
    );
    enqueue(
      &mut *tx,
      kind::BACK_IN_STOCK,
      &alert.email_norm,
      &email,
      Some(&format!("stock-alert:{}", alert.id)),
    )
    .await?;
    ids.push(alert.id);
  }
  sqlx::query("UPDATE stock_alerts SET notified_at = now() WHERE id = ANY($1)")
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
  tx.commit().await?;
  Ok(ids.len() as u64)
}

#[cfg(test)]
mod tests {
  use super::*;

  async fn seed_variant(pool: &PgPool, stock: i32) -> (Uuid, Uuid) {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let warehouse_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents)
       VALUES ($1, 'Biquíni Sol', $2, 9990)",
    )
    .bind(product_id)
    .bind(format!("biquini-{}", product_id.simple()))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'm', 'Azul')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("sku-{variant_id}"))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO warehouses (id, code, name, is_default, active)
       VALUES ($1, $2, 'Principal', true, true)",
    )
    .bind(warehouse_id)
    .bind(format!("wh-{}", warehouse_id.simple()))
    .execute(pool)
    .await
    .unwrap();
    let inventory_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
       VALUES ($1, $2, $3, $4, 0)",
    )
    .bind(inventory_id)
    .bind(variant_id)
    .bind(warehouse_id)
    .bind(stock)
    .execute(pool)
    .await
    .unwrap();
    (variant_id, inventory_id)
  }

  async fn set_stock(pool: &PgPool, inventory_id: Uuid, quantity: i32) {
    sqlx::query("UPDATE inventory SET quantity = $2 WHERE id = $1")
      .bind(inventory_id)
      .bind(quantity)
      .execute(pool)
      .await
      .unwrap();
  }

  async fn mails_to(pool: &PgPool, to: &str) -> Vec<(String, String)> {
    sqlx::query_as(
      "SELECT kind, text_body FROM email_outbox WHERE to_address = $1 ORDER BY created_at",
    )
    .bind(to)
    .fetch_all(pool)
    .await
    .unwrap()
  }

  #[tokio::test]
  async fn subscribe_validates_and_dedupes() {
    let pool = crate::test_support::isolated_pool().await;
    let (sold_out, _) = seed_variant(&pool, 0).await;
    let (in_stock, _) = seed_variant(&pool, 3).await;

    assert_eq!(
      subscribe(&pool, sold_out, "ana@example.com", None)
        .await
        .unwrap(),
      SubscribeOutcome::Subscribed
    );
    assert_eq!(
      subscribe(&pool, sold_out, "ana@example.com", None)
        .await
        .unwrap(),
      SubscribeOutcome::AlreadySubscribed
    );
    assert_eq!(
      subscribe(&pool, in_stock, "ana@example.com", None)
        .await
        .unwrap(),
      SubscribeOutcome::InStock
    );
    assert_eq!(
      subscribe(&pool, Uuid::now_v7(), "ana@example.com", None)
        .await
        .unwrap(),
      SubscribeOutcome::UnknownVariant
    );
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM stock_alerts")
      .fetch_one(&pool)
      .await
      .unwrap();
    assert_eq!(rows, 1);

    // Per-address cap.
    for _ in 1..MAX_PENDING_PER_EMAIL {
      let (variant, _) = seed_variant(&pool, 0).await;
      assert_eq!(
        subscribe(&pool, variant, "ana@example.com", None)
          .await
          .unwrap(),
        SubscribeOutcome::Subscribed
      );
    }
    let (one_more, _) = seed_variant(&pool, 0).await;
    assert_eq!(
      subscribe(&pool, one_more, "ana@example.com", None)
        .await
        .unwrap(),
      SubscribeOutcome::TooMany
    );
    assert_eq!(
      subscribe(&pool, one_more, "bia@example.com", None)
        .await
        .unwrap(),
      SubscribeOutcome::Subscribed
    );
  }

  #[tokio::test]
  async fn restock_mails_each_alert_exactly_once() {
    let pool = crate::test_support::isolated_pool().await;
    let (variant, inventory) = seed_variant(&pool, 0).await;
    for email in ["ana@example.com", "bia@example.com"] {
      subscribe(&pool, variant, email, None).await.unwrap();
    }
    // Still sold out: nothing to send.
    assert_eq!(notify_restocked(&pool).await.unwrap(), 0);

    set_stock(&pool, inventory, 2).await;
    assert_eq!(notify_restocked(&pool).await.unwrap(), 2);
    assert_eq!(notify_restocked(&pool).await.unwrap(), 0);
    let mails = mails_to(&pool, "ana@example.com").await;
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].0, kind::BACK_IN_STOCK);
    assert!(
      mails[0].1.contains("Biquíni Sol (M · Azul)"),
      "{}",
      mails[0].1
    );
    assert!(mails[0].1.contains(&format!("?variant={variant}")));
    assert!(mails[0].1.contains(&cancel_url("ana@example.com")));

    // Sold out and restocked again: closed alerts never re-send.
    set_stock(&pool, inventory, 0).await;
    assert_eq!(notify_restocked(&pool).await.unwrap(), 0);
    set_stock(&pool, inventory, 5).await;
    assert_eq!(notify_restocked(&pool).await.unwrap(), 0);
    assert_eq!(mails_to(&pool, "ana@example.com").await.len(), 1);
    assert_eq!(mails_to(&pool, "bia@example.com").await.len(), 1);

    // A fresh subscription after the notice is a new alert.
    set_stock(&pool, inventory, 0).await;
    assert_eq!(
      subscribe(&pool, variant, "ana@example.com", None)
        .await
        .unwrap(),
      SubscribeOutcome::Subscribed
    );
    set_stock(&pool, inventory, 1).await;
    assert_eq!(notify_restocked(&pool).await.unwrap(), 1);
    assert_eq!(mails_to(&pool, "ana@example.com").await.len(), 2);
  }

  #[tokio::test]
  async fn inactive_products_and_held_stock_do_not_trigger() {
    let pool = crate::test_support::isolated_pool().await;
    let (variant, inventory) = seed_variant(&pool, 0).await;
    subscribe(&pool, variant, "ana@example.com", None)
      .await
      .unwrap();
    // On hand but fully reserved by a live hold: still sold out.
    sqlx::query(
      "UPDATE inventory SET quantity = 2, reserved = 2 WHERE id = $1",
    )
    .bind(inventory)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(notify_restocked(&pool).await.unwrap(), 0);
    // Sellable but the product is hidden.
    set_stock(&pool, inventory, 5).await;
    sqlx::query(
      "UPDATE products SET active = false
       WHERE id = (SELECT product_id FROM product_variants WHERE id = $1)",
    )
    .bind(variant)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(notify_restocked(&pool).await.unwrap(), 0);
    assert!(mails_to(&pool, "ana@example.com").await.is_empty());
  }

  #[tokio::test]
  async fn cancel_link_drops_pending_alerts() {
    let pool = crate::test_support::isolated_pool().await;
    let (variant, inventory) = seed_variant(&pool, 0).await;
    subscribe(&pool, variant, "ana@example.com", None)
      .await
      .unwrap();
    let url = cancel_url("Ana@Example.com");
    assert!(url.contains("email=ana%40example.com&token="));
    let token = url.rsplit_once("token=").unwrap().1;
    assert!(verify_cancel_token("ana@example.com", token));
    // Domain-separated from the newsletter unsubscribe token.
    assert!(!crate::app::newsletter::verify_signed_email_token(
      "newsletter-unsubscribe:v1:",
      "ana@example.com",
      token
    ));
    assert_eq!(cancel_all(&pool, "ANA@example.com").await.unwrap(), 1);
    set_stock(&pool, inventory, 3).await;
    assert_eq!(notify_restocked(&pool).await.unwrap(), 0);
  }
}
