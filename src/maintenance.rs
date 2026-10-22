//! Periodic data hygiene, run from the background sweeper in `src/main.rs`.
//!
//! Guest carts and verification rows can be created without an account, so
//! nothing else ever deletes most of them. Each pass deletes at most
//! [`SWEEP_BATCH`] rows per table so a large backlog drains over a few
//! ticks instead of one long-running statement.

use sqlx::PgPool;

/// Max rows deleted per table per pass.
pub const SWEEP_BATCH: i64 = 1000;

/// Rows removed by one [`sweep_stale_rows`] pass.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepOutcome {
  pub carts: u64,
  pub verifications: u64,
}

/// Delete stale guest carts and expired verification rows.
///
/// Guest carts (`user_id IS NULL`) go when untouched for 30 days (the cart
/// cookie's lifetime, so no browser can still point at them) or when empty
/// and untouched for a day (abandoned/bot carts). `cart_items` cascade.
/// Signed-in users' carts are never swept. Verifications (reset, verify,
/// OAuth state, pending 2FA, ...) are useless once `expires_at` passes.
pub async fn sweep_stale_rows(
  pool: &PgPool,
) -> Result<SweepOutcome, sqlx::Error> {
  let carts = sqlx::query(
    "DELETE FROM carts WHERE id IN (
       SELECT c.id FROM carts c
       WHERE c.user_id IS NULL
         AND (
           c.updated_at < now() - interval '30 days'
           OR (
             c.updated_at < now() - interval '1 day'
             AND NOT EXISTS (SELECT 1 FROM cart_items i WHERE i.cart_id = c.id)
           )
         )
       LIMIT $1
     )",
  )
  .bind(SWEEP_BATCH)
  .execute(pool)
  .await?
  .rows_affected();

  let verifications = sqlx::query(
    "DELETE FROM verifications WHERE id IN (
       SELECT id FROM verifications WHERE expires_at < now() LIMIT $1
     )",
  )
  .bind(SWEEP_BATCH)
  .execute(pool)
  .await?
  .rows_affected();

  Ok(SweepOutcome {
    carts,
    verifications,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use uuid::Uuid;

  async fn insert_cart(
    pool: &PgPool,
    user_id: Option<Uuid>,
    age: &str,
  ) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(&format!(
      "INSERT INTO carts (id, token, user_id, created_at, updated_at)
       VALUES ($1, $2, $3, now() - interval '{age}', now() - interval '{age}')"
    ))
    .bind(id)
    .bind(Uuid::now_v7())
    .bind(user_id)
    .execute(pool)
    .await
    .expect("insert cart");
    id
  }

  async fn add_line(pool: &PgPool, cart_id: Uuid) {
    let product_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents)
       VALUES ($1, 'Peça teste', $2, 1000)",
    )
    .bind(product_id)
    .bind(format!("sweep-{product_id}"))
    .execute(pool)
    .await
    .expect("insert product");
    let variant_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'm', 'azul')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("SWEEP-{variant_id}"))
    .execute(pool)
    .await
    .expect("insert variant");
    sqlx::query(
      "INSERT INTO cart_items (id, cart_id, variant_id, quantity)
       VALUES ($1, $2, $3, 1)",
    )
    .bind(Uuid::now_v7())
    .bind(cart_id)
    .bind(variant_id)
    .execute(pool)
    .await
    .expect("insert cart item");
  }

  async fn insert_user(pool: &PgPool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, 'Sweep', $2)")
      .bind(id)
      .bind(format!("sweep-{id}@example.com"))
      .execute(pool)
      .await
      .expect("insert user");
    id
  }

  async fn exists(pool: &PgPool, table: &str, id: Uuid) -> bool {
    sqlx::query_scalar(&format!(
      "SELECT EXISTS(SELECT 1 FROM {table} WHERE id = $1)"
    ))
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("exists")
  }

  async fn insert_verification(pool: &PgPool, expires: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(&format!(
      "INSERT INTO verifications (id, identifier, value, expires_at)
       VALUES ($1, $2, '', now() {expires})"
    ))
    .bind(id)
    .bind(format!("sweep-test:{id}"))
    .execute(pool)
    .await
    .expect("insert verification");
    id
  }

  #[tokio::test]
  async fn sweeps_stale_guest_carts_and_expired_verifications() {
    // Own database: the sweep deletes every stale row, not just ours.
    let pool = crate::test_support::isolated_pool().await;

    let fresh_empty = insert_cart(&pool, None, "1 hour").await;
    let stale_empty = insert_cart(&pool, None, "2 days").await;
    let recent_full = insert_cart(&pool, None, "2 days").await;
    add_line(&pool, recent_full).await;
    let abandoned_full = insert_cart(&pool, None, "31 days").await;
    add_line(&pool, abandoned_full).await;
    let user = insert_user(&pool).await;
    let user_cart = insert_cart(&pool, Some(user), "90 days").await;

    let expired = insert_verification(&pool, "- interval '1 minute'").await;
    let live = insert_verification(&pool, "+ interval '1 hour'").await;

    let outcome = sweep_stale_rows(&pool).await.expect("sweep");
    assert_eq!(
      outcome,
      SweepOutcome {
        carts: 2,
        verifications: 1
      }
    );

    assert!(exists(&pool, "carts", fresh_empty).await);
    assert!(!exists(&pool, "carts", stale_empty).await);
    assert!(exists(&pool, "carts", recent_full).await);
    assert!(!exists(&pool, "carts", abandoned_full).await);
    assert!(exists(&pool, "carts", user_cart).await);
    assert!(!exists(&pool, "verifications", expired).await);
    assert!(exists(&pool, "verifications", live).await);
  }
}
