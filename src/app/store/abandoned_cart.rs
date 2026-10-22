//! Abandoned-cart reminders, sent from the background sweeper in
//! `src/main.rs` ([`send_reminders`]).
//!
//! A cart is eligible when it holds at least one line of an active product,
//! its last activity (`carts.updated_at` or any line's `updated_at`) is
//! older than `ABANDONED_CART_AFTER_MINUTES` but within [`MAX_IDLE_DAYS`]
//! (ancient carts are left alone, e.g. on first deploy), it has a known
//! address (verified account email, else the one typed on the checkout
//! page, `carts.contact_email`), no order came from it (or from its
//! account) since that activity, and the address has not unsubscribed from
//! marketing (`newsletter_subscribers.unsubscribed_at`).
//!
//! Caps: one reminder per abandonment (a cart is only reminded again after
//! new activity), at most one per cart per [`REMIND_EVERY_DAYS`], and at
//! most one per *address* per [`REMIND_EVERY_DAYS`] — the checkout email
//! field is unauthenticated, so a stranger typing someone else's address on
//! many carts still yields one mail a week at most.

use sqlx::PgPool;
use uuid::Uuid;

use crate::app::store::queries::size_label;
use crate::mail::outbox::{enqueue, kind};
use crate::mail::templates::{self, CartReminderLine};
use crate::mail::{absolute_url, redact_address};

/// Minimum days between two reminders of one cart, or to one address.
pub const REMIND_EVERY_DAYS: i64 = 7;
/// Carts idle for longer than this are never reminded.
pub const MAX_IDLE_DAYS: i64 = 7;
/// Carts considered per sweep pass.
const SWEEP_BATCH: i64 = 100;

/// Remember the address typed on the checkout page for a later reminder.
/// Does not count as cart activity (`updated_at` is left alone).
pub async fn remember_checkout_email(
  pool: &PgPool,
  cart_id: Uuid,
  email_norm: &str,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "UPDATE carts SET contact_email = $2
     WHERE id = $1 AND contact_email IS DISTINCT FROM $2",
  )
  .bind(cart_id)
  .bind(email_norm)
  .execute(pool)
  .await?;
  Ok(())
}

#[derive(sqlx::FromRow)]
struct Candidate {
  id: Uuid,
  email: String,
  name: Option<String>,
  reminded_at: Option<time::OffsetDateTime>,
}

#[derive(sqlx::FromRow)]
struct LineRow {
  product_name: String,
  size: String,
  color: String,
  quantity: i32,
  unit_price_cents: i64,
}

/// Eligible carts (see the module docs). `$1` = idle minutes, `$2` =
/// remind/idle window in days, `$3` = batch size, `$4` = the reminder
/// outbox kind.
const CANDIDATES_SQL: &str = r#"
WITH activity AS (
    SELECT c.id,
           c.user_id,
           c.abandoned_reminded_at,
           GREATEST(
               c.updated_at,
               (SELECT max(ci.updated_at) FROM cart_items ci WHERE ci.cart_id = c.id)
           ) AS last_activity,
           lower(btrim(COALESCE(
               CASE WHEN u.email_verified THEN u.email END,
               c.contact_email
           ))) AS email,
           u.name
    FROM carts c
    LEFT JOIN users u ON u.id = c.user_id
    WHERE EXISTS (
        SELECT 1 FROM cart_items ci
        INNER JOIN product_variants pv ON pv.id = ci.variant_id
        INNER JOIN products p ON p.id = pv.product_id
        WHERE ci.cart_id = c.id AND p.active = true
    )
)
SELECT a.id, a.email, a.name, a.abandoned_reminded_at AS reminded_at
FROM activity a
WHERE a.email IS NOT NULL AND a.email <> ''
  AND a.last_activity < now() - make_interval(mins => $1::int)
  AND a.last_activity > now() - make_interval(days => $2::int)
  AND (
      a.abandoned_reminded_at IS NULL
      OR (a.abandoned_reminded_at < a.last_activity
          AND a.abandoned_reminded_at < now() - make_interval(days => $2::int))
  )
  AND NOT EXISTS (
      SELECT 1 FROM orders o
      WHERE (o.cart_id = a.id OR (a.user_id IS NOT NULL AND o.user_id = a.user_id))
        AND o.created_at >= a.last_activity
  )
  AND NOT EXISTS (
      SELECT 1 FROM newsletter_subscribers ns
      WHERE lower(ns.email) = a.email AND ns.unsubscribed_at IS NOT NULL
  )
  AND NOT EXISTS (
      SELECT 1 FROM email_outbox eo
      WHERE eo.kind = $4 AND lower(eo.to_address) = a.email
        AND eo.created_at > now() - make_interval(days => $2::int)
  )
ORDER BY a.last_activity
LIMIT $3
"#;

/// Sweeper pass: queue one reminder per eligible cart. Returns the number
/// queued. Each cart is claimed with a compare-and-set on
/// `abandoned_reminded_at` in its own short transaction, so replicas
/// running the same pass never double-send.
pub async fn send_reminders(
  pool: &PgPool,
  after_minutes: i64,
) -> Result<u64, sqlx::Error> {
  let candidates: Vec<Candidate> = sqlx::query_as(CANDIDATES_SQL)
    .bind(after_minutes.clamp(1, i64::from(i32::MAX)) as i32)
    .bind(REMIND_EVERY_DAYS.max(MAX_IDLE_DAYS) as i32)
    .bind(SWEEP_BATCH)
    .bind(kind::ABANDONED_CART)
    .fetch_all(pool)
    .await?;

  let mut sent = 0;
  for candidate in candidates {
    let mut tx = pool.begin().await?;
    let claimed = sqlx::query(
      "UPDATE carts SET abandoned_reminded_at = now()
       WHERE id = $1 AND abandoned_reminded_at IS NOT DISTINCT FROM $2",
    )
    .bind(candidate.id)
    .bind(candidate.reminded_at)
    .execute(&mut *tx)
    .await?
    .rows_affected()
      > 0;
    // Same address on two carts in one batch: the second sees the first's
    // queued mail.
    let recent: bool = sqlx::query_scalar(
      "SELECT EXISTS (
         SELECT 1 FROM email_outbox
         WHERE kind = $1 AND lower(to_address) = $2
           AND created_at > now() - make_interval(days => $3::int)
       )",
    )
    .bind(kind::ABANDONED_CART)
    .bind(&candidate.email)
    .bind(REMIND_EVERY_DAYS as i32)
    .fetch_one(&mut *tx)
    .await?;
    if !claimed || recent {
      tx.rollback().await?;
      continue;
    }
    let lines: Vec<LineRow> = sqlx::query_as(
      "SELECT p.name AS product_name, pv.size::text AS size, pv.color,
              ci.quantity,
              COALESCE(pv.price_in_cents, p.price_in_cents)::bigint AS unit_price_cents
       FROM cart_items ci
       INNER JOIN product_variants pv ON pv.id = ci.variant_id
       INNER JOIN products p ON p.id = pv.product_id
       WHERE ci.cart_id = $1 AND p.active = true
       ORDER BY ci.created_at, ci.id",
    )
    .bind(candidate.id)
    .fetch_all(&mut *tx)
    .await?;
    let lines: Vec<CartReminderLine> = lines
      .into_iter()
      .map(|row| CartReminderLine {
        product_name: row.product_name,
        variant_label: format!("{} · {}", size_label(&row.size), row.color),
        quantity: row.quantity,
        unit_price_cents: row.unit_price_cents,
      })
      .collect();
    if lines.is_empty() {
      tx.rollback().await?;
      continue;
    }
    let email = templates::abandoned_cart(
      candidate.name.as_deref().unwrap_or(""),
      &lines,
      &absolute_url("/checkout"),
      &crate::app::newsletter::unsubscribe_url(&candidate.email),
    );
    enqueue(
      &mut *tx,
      kind::ABANDONED_CART,
      &candidate.email,
      &email,
      None,
    )
    .await?;
    tx.commit().await?;
    tracing::debug!(
      cart_id = %candidate.id,
      to = %redact_address(&candidate.email),
      "abandoned-cart reminder queued"
    );
    sent += 1;
  }
  Ok(sent)
}

#[cfg(test)]
mod tests {
  use super::*;

  async fn seed_variant(pool: &PgPool) -> Uuid {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents)
       VALUES ($1, 'Maiô Mar', $2, 15990)",
    )
    .bind(product_id)
    .bind(format!("maio-{}", product_id.simple()))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'g', 'Verde')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("sku-{variant_id}"))
    .execute(pool)
    .await
    .unwrap();
    variant_id
  }

  /// Cart idle for `idle` (SQL interval), optionally with a guest email,
  /// holding one line of `variant_id` (none when `None`).
  async fn seed_cart(
    pool: &PgPool,
    variant_id: Option<Uuid>,
    idle: &str,
    contact_email: Option<&str>,
    user_id: Option<Uuid>,
  ) -> Uuid {
    let cart_id = Uuid::now_v7();
    sqlx::query(&format!(
      "INSERT INTO carts (id, token, user_id, contact_email, created_at, updated_at)
       VALUES ($1, $2, $3, $4, now() - interval '{idle}', now() - interval '{idle}')"
    ))
    .bind(cart_id)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(contact_email)
    .execute(pool)
    .await
    .unwrap();
    if let Some(variant_id) = variant_id {
      sqlx::query(&format!(
        "INSERT INTO cart_items (id, cart_id, variant_id, quantity, created_at, updated_at)
         VALUES ($1, $2, $3, 2, now() - interval '{idle}', now() - interval '{idle}')"
      ))
      .bind(Uuid::now_v7())
      .bind(cart_id)
      .bind(variant_id)
      .execute(pool)
      .await
      .unwrap();
    }
    cart_id
  }

  async fn reminders(pool: &PgPool) -> Vec<(String, String)> {
    sqlx::query_as(
      "SELECT to_address, text_body FROM email_outbox
       WHERE kind = 'abandoned_cart' ORDER BY to_address",
    )
    .fetch_all(pool)
    .await
    .unwrap()
  }

  #[tokio::test]
  async fn reminds_only_eligible_carts() {
    let pool = crate::test_support::isolated_pool().await;
    let variant = seed_variant(&pool).await;

    // Eligible guest cart.
    seed_cart(
      &pool,
      Some(variant),
      "4 hours",
      Some("ok@example.com"),
      None,
    )
    .await;
    // Empty cart.
    seed_cart(&pool, None, "4 hours", Some("empty@example.com"), None).await;
    // Touched too recently.
    seed_cart(
      &pool,
      Some(variant),
      "1 hour",
      Some("recent@example.com"),
      None,
    )
    .await;
    // Idle for too long.
    seed_cart(
      &pool,
      Some(variant),
      "10 days",
      Some("old@example.com"),
      None,
    )
    .await;
    // No known address.
    seed_cart(&pool, Some(variant), "4 hours", None, None).await;
    // Converted: an order came from this cart after its last activity.
    let converted = seed_cart(
      &pool,
      Some(variant),
      "4 hours",
      Some("bought@example.com"),
      None,
    )
    .await;
    sqlx::query(
      "INSERT INTO orders (id, guest_email, cart_id, created_at)
       VALUES ($1, 'bought@example.com', $2, now() - interval '3 hours')",
    )
    .bind(Uuid::now_v7())
    .bind(converted)
    .execute(&pool)
    .await
    .unwrap();
    // Unsubscribed from marketing (suppression row for a non-subscriber).
    seed_cart(
      &pool,
      Some(variant),
      "4 hours",
      Some("out@example.com"),
      None,
    )
    .await;
    crate::app::newsletter::unsubscribe_email(&pool, "OUT@example.com")
      .await
      .unwrap();
    // Signed-in owner with a verified email (no contact email needed).
    let user_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO users (id, name, email, email_verified)
       VALUES ($1, 'Ana', 'ana@example.com', true)",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    seed_cart(&pool, Some(variant), "5 hours", None, Some(user_id)).await;

    assert_eq!(send_reminders(&pool, 180).await.unwrap(), 2);
    let mails = reminders(&pool).await;
    let to: Vec<&str> = mails.iter().map(|(to, _)| to.as_str()).collect();
    assert_eq!(to, ["ana@example.com", "ok@example.com"]);
    let body = &mails[1].1;
    assert!(body.contains("Maiô Mar — G · Verde (x2) — R$"), "{body}");
    assert!(body.contains("/checkout"));
    assert!(
      body.contains(&crate::app::newsletter::unsubscribe_url("ok@example.com"))
    );
    assert!(mails[0].1.contains("Olá, Ana!"));

    // Never twice for the same abandonment.
    assert_eq!(send_reminders(&pool, 180).await.unwrap(), 0);
    assert_eq!(reminders(&pool).await.len(), 2);
  }

  #[tokio::test]
  async fn at_most_one_reminder_per_cart_and_address_per_week() {
    let pool = crate::test_support::isolated_pool().await;
    let variant = seed_variant(&pool).await;
    let cart = seed_cart(
      &pool,
      Some(variant),
      "4 hours",
      Some("ana@example.com"),
      None,
    )
    .await;
    // Same address on a second cart: one mail for both.
    seed_cart(
      &pool,
      Some(variant),
      "5 hours",
      Some("ana@example.com"),
      None,
    )
    .await;
    assert_eq!(send_reminders(&pool, 180).await.unwrap(), 1);

    // New activity on the reminded cart, abandoned again: still capped by
    // the 7-day window.
    sqlx::query(
      "UPDATE carts SET abandoned_reminded_at = now() - interval '2 days',
                        updated_at = now() - interval '4 hours'
       WHERE id = $1",
    )
    .bind(cart)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
      "UPDATE email_outbox SET created_at = now() - interval '2 days'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(send_reminders(&pool, 180).await.unwrap(), 0);

    // A week later it may go again.
    sqlx::query(
      "UPDATE carts SET abandoned_reminded_at = now() - interval '8 days'
       WHERE id = $1",
    )
    .bind(cart)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
      "UPDATE email_outbox SET created_at = now() - interval '8 days'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(send_reminders(&pool, 180).await.unwrap(), 1);
    assert_eq!(reminders(&pool).await.len(), 2);
  }

  #[tokio::test]
  async fn checkout_email_is_remembered_without_counting_as_activity() {
    let pool = crate::test_support::isolated_pool().await;
    let variant = seed_variant(&pool).await;
    let cart = seed_cart(&pool, Some(variant), "4 hours", None, None).await;
    assert_eq!(send_reminders(&pool, 180).await.unwrap(), 0);
    remember_checkout_email(&pool, cart, "guest@example.com")
      .await
      .unwrap();
    assert_eq!(send_reminders(&pool, 180).await.unwrap(), 1);
    assert_eq!(reminders(&pool).await[0].0, "guest@example.com");
  }
}
