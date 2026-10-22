//! Stock holds and movements on `inventory` rows.
//!
//! **Lock order invariant:** every transaction that writes `inventory` takes
//! all the row locks it needs in one statement, `… ORDER BY i.id FOR UPDATE`
//! (see [`lock_inventory_ids`] / [`lock_variant_inventory`]), before the first
//! write. Later locks in the same transaction only re-take rows it already
//! holds. Since everyone acquires in ascending `id` order, pay, cancel, the
//! sweeper, checkout and admin adjustments on overlapping rows queue instead
//! of deadlocking (`40P01`). Keep it that way when adding a code path.

use std::collections::HashMap;

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::app::store::order::{ApplyError, ApplyOptions, OrderEvent, apply};

/// Sellable units of the inventory row aliased `i`, for display and cart
/// clamps. Holds of pending orders whose `reservation_expires_at` has passed
/// stay in `i.reserved` until the background sweeper releases them; they are
/// treated as free here so stock doesn't look sold out between sweeps.
/// Checkout reclaims those holds for real in [`lock_cart_inventory`].
pub(crate) const LIVE_AVAILABLE_SQL: &str =
  "GREATEST(i.quantity - GREATEST(i.reserved - COALESCE((
    SELECT SUM(r.quantity) FROM inventory_reservations r
    INNER JOIN orders o ON o.id = r.order_id
    WHERE r.inventory_id = i.id
      AND o.status = 'pending_payment'
      AND o.reservation_expires_at < now()), 0), 0), 0)";

#[derive(Debug)]
pub enum InventoryError {
  Insufficient,
  Db(sqlx::Error),
}

impl From<sqlx::Error> for InventoryError {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

impl std::fmt::Display for InventoryError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Insufficient => {
        write!(f, "Estoque insuficiente para concluir o pedido.")
      }
      Self::Db(error) => write!(f, "{error}"),
    }
  }
}

pub struct ReserveLine {
  pub variant_id: Uuid,
  pub quantity: i32,
}

#[derive(sqlx::FromRow)]
struct InventoryLockRow {
  id: Uuid,
  variant_id: Uuid,
  quantity: i32,
  reserved: i32,
  is_default: bool,
}

#[derive(sqlx::FromRow)]
struct ReservationRow {
  inventory_id: Uuid,
  quantity: i32,
}

/// Lock the given inventory rows (any warehouse) in `id` order.
async fn lock_inventory_ids(
  tx: &mut Transaction<'_, Postgres>,
  ids: &[Uuid],
) -> Result<Vec<InventoryLockRow>, sqlx::Error> {
  let mut ids = ids.to_vec();
  ids.sort_unstable();
  ids.dedup();
  if ids.is_empty() {
    return Ok(Vec::new());
  }
  sqlx::query_as::<_, InventoryLockRow>(
    r#"
        SELECT i.id, i.variant_id, i.quantity, i.reserved, w.is_default
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.id = ANY($1)
        ORDER BY i.id
        FOR UPDATE OF i
        "#,
  )
  .bind(&ids)
  .fetch_all(&mut **tx)
  .await
}

/// Lock every active-warehouse inventory row of the given variants, in `id`
/// order, in one statement.
async fn lock_variant_inventory(
  tx: &mut Transaction<'_, Postgres>,
  variant_ids: &[Uuid],
) -> Result<Vec<InventoryLockRow>, sqlx::Error> {
  let mut variant_ids = variant_ids.to_vec();
  variant_ids.sort_unstable();
  variant_ids.dedup();
  if variant_ids.is_empty() {
    return Ok(Vec::new());
  }
  sqlx::query_as::<_, InventoryLockRow>(
    r#"
        SELECT i.id, i.variant_id, i.quantity, i.reserved, w.is_default
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id = ANY($1) AND w.active = true
        ORDER BY i.id
        FOR UPDATE OF i
        "#,
  )
  .bind(&variant_ids)
  .fetch_all(&mut **tx)
  .await
}

/// Lock every active-warehouse inventory row for the cart, plus every row
/// held by an expired-but-unswept pending order that blocks one of them, in
/// one `id`-ordered statement. Then cancel those expired orders (skipping any
/// another transaction is already handling) so their holds are free for this
/// checkout. Checkout quoting and reserve share this one lock set.
pub async fn lock_cart_inventory(
  tx: &mut Transaction<'_, Postgres>,
  cart_id: Uuid,
) -> Result<(), sqlx::Error> {
  let cart_rows: Vec<Uuid> = sqlx::query_scalar(
    r#"
        SELECT i.id
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        INNER JOIN cart_items ci ON ci.variant_id = i.variant_id
        WHERE ci.cart_id = $1 AND w.active = true
        "#,
  )
  .bind(cart_id)
  .fetch_all(&mut **tx)
  .await?;
  if cart_rows.is_empty() {
    return Ok(());
  }

  // Expired holders of the cart's rows, and every row they hold (a cancel
  // releases all of them, so they must be inside the one sorted lock).
  let expired: Vec<(Uuid, Uuid)> = sqlx::query_as(
    r#"
        SELECT r.order_id, r.inventory_id
        FROM inventory_reservations r
        WHERE r.order_id IN (
            SELECT r2.order_id
            FROM inventory_reservations r2
            INNER JOIN orders o ON o.id = r2.order_id
            WHERE r2.inventory_id = ANY($1)
              AND o.status = 'pending_payment'
              AND o.reservation_expires_at < now()
        )
        "#,
  )
  .bind(&cart_rows)
  .fetch_all(&mut **tx)
  .await?;

  let mut ids = cart_rows;
  ids.extend(expired.iter().map(|(_, inventory_id)| *inventory_id));
  lock_inventory_ids(tx, &ids).await?;

  let mut order_ids: Vec<Uuid> =
    expired.into_iter().map(|(order_id, _)| order_id).collect();
  order_ids.sort_unstable();
  order_ids.dedup();
  if order_ids.is_empty() {
    return Ok(());
  }
  // SKIP LOCKED: never wait on an order row while holding inventory locks
  // (`apply` takes order → inventory). A skipped order is being paid,
  // cancelled or swept by someone else right now.
  let claimed: Vec<Uuid> = sqlx::query_scalar(
    r#"
        SELECT id FROM orders
        WHERE id = ANY($1)
          AND status = 'pending_payment'
          AND reservation_expires_at < now()
        ORDER BY id
        FOR UPDATE SKIP LOCKED
        "#,
  )
  .bind(&order_ids)
  .fetch_all(&mut **tx)
  .await?;
  for order_id in claimed {
    match apply(tx, order_id, OrderEvent::Cancel, ApplyOptions::default()).await
    {
      Ok(_) => {}
      Err(ApplyError::Db(error)) => return Err(error),
      // Locked above as pending + expired, so cancel can't be illegal; if
      // it ever is, no statement failed and the sweeper retries it later.
      Err(error) => {
        tracing::warn!(%order_id, %error, "expired hold reclaim skipped");
      }
    }
  }
  Ok(())
}

/// Hold stock for a pending PIX order: `reserved += qty` across warehouses
/// (default first) and insert one [`inventory_reservations`] row per take.
pub async fn reserve_for_order(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  lines: &[ReserveLine],
) -> Result<(), InventoryError> {
  let variant_ids: Vec<Uuid> = lines
    .iter()
    .filter(|line| line.quantity > 0)
    .map(|line| line.variant_id)
    .collect();
  let mut rows = lock_variant_inventory(tx, &variant_ids).await?;
  // Default warehouse first, then `id`. Locks were already taken in `id`
  // order above; this only picks which rows each line draws from.
  rows.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.id.cmp(&b.id)));

  for line in lines {
    if line.quantity <= 0 {
      continue;
    }
    let available: i32 = rows
      .iter()
      .filter(|row| row.variant_id == line.variant_id)
      .map(|row| (row.quantity - row.reserved).max(0))
      .sum();
    if available < line.quantity {
      return Err(InventoryError::Insufficient);
    }
    let mut remaining = line.quantity;

    for row in rows
      .iter_mut()
      .filter(|row| row.variant_id == line.variant_id)
    {
      if remaining <= 0 {
        break;
      }
      let avail = (row.quantity - row.reserved).max(0);
      let take = remaining.min(avail);
      if take <= 0 {
        continue;
      }
      sqlx::query(
        "UPDATE inventory SET reserved = reserved + $2, updated_at = now() WHERE id = $1",
      )
      .bind(row.id)
      .bind(take)
      .execute(&mut **tx)
      .await?;
      sqlx::query(
        "INSERT INTO inventory_reservations (id, order_id, inventory_id, quantity)
         VALUES ($1, $2, $3, $4)",
      )
      .bind(Uuid::now_v7())
      .bind(order_id)
      .bind(row.id)
      .bind(take)
      .execute(&mut **tx)
      .await?;
      // The same variant can appear on several lines: track what this
      // transaction already took.
      row.reserved += take;
      remaining -= take;
    }
    if remaining > 0 {
      return Err(InventoryError::Insufficient);
    }
  }

  sqlx::query(
    "UPDATE orders SET reservation_expires_at = now() + interval '48 hours' WHERE id = $1",
  )
  .bind(order_id)
  .execute(&mut **tx)
  .await?;

  Ok(())
}

async fn reservation_rows(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<Vec<ReservationRow>, sqlx::Error> {
  sqlx::query_as::<_, ReservationRow>(
    "SELECT inventory_id, quantity FROM inventory_reservations WHERE order_id = $1",
  )
  .bind(order_id)
  .fetch_all(&mut **tx)
  .await
}

async fn apply_reservation_rows(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  rows: &[ReservationRow],
  convert: bool,
) -> Result<(), sqlx::Error> {
  let ids: Vec<Uuid> = rows.iter().map(|row| row.inventory_id).collect();
  let locked = lock_inventory_ids(tx, &ids).await?;
  // inventory id → (variant id, on-hand quantity as this loop updates it).
  let mut on_hand: HashMap<Uuid, (Uuid, i32)> = locked
    .iter()
    .map(|row| (row.id, (row.variant_id, row.quantity)))
    .collect();

  for row in rows {
    if convert {
      if let Some((variant_id, quantity)) = on_hand.get_mut(&row.inventory_id) {
        if *quantity < row.quantity {
          tracing::warn!(
            %order_id,
            %variant_id,
            inventory_id = %row.inventory_id,
            on_hand = *quantity,
            sold = row.quantity,
            "convert clamps stock at zero: oversell"
          );
        }
        *quantity = (*quantity - row.quantity).max(0);
      }
      sqlx::query(
        "UPDATE inventory
         SET reserved = GREATEST(reserved - $2, 0),
             quantity = GREATEST(quantity - $2, 0),
             updated_at = now()
         WHERE id = $1",
      )
      .bind(row.inventory_id)
      .bind(row.quantity)
      .execute(&mut **tx)
      .await?;
    } else {
      sqlx::query(
        "UPDATE inventory
         SET reserved = GREATEST(reserved - $2, 0), updated_at = now()
         WHERE id = $1",
      )
      .bind(row.inventory_id)
      .bind(row.quantity)
      .execute(&mut **tx)
      .await?;
    }
  }
  Ok(())
}

async fn clear_reservation_rows(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query("DELETE FROM inventory_reservations WHERE order_id = $1")
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
  Ok(())
}

/// Convert a hold into a sale using the stored warehouse rows.
pub async fn convert_reservation(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<(), sqlx::Error> {
  let rows = reservation_rows(tx, order_id).await?;
  if rows.is_empty() {
    fallback_from_order_items(tx, order_id, true).await?;
  } else {
    apply_reservation_rows(tx, order_id, &rows, true).await?;
    clear_reservation_rows(tx, order_id).await?;
  }
  sqlx::query("UPDATE orders SET reservation_expires_at = NULL WHERE id = $1")
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
  Ok(())
}

/// Release a hold without selling, using the stored warehouse rows.
pub async fn release_reservation(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<(), sqlx::Error> {
  let rows = reservation_rows(tx, order_id).await?;
  if rows.is_empty() {
    fallback_from_order_items(tx, order_id, false).await?;
  } else {
    apply_reservation_rows(tx, order_id, &rows, false).await?;
    clear_reservation_rows(tx, order_id).await?;
  }
  sqlx::query("UPDATE orders SET reservation_expires_at = NULL WHERE id = $1")
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
  Ok(())
}

async fn order_item_quantities(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<Vec<(Uuid, i32)>, sqlx::Error> {
  sqlx::query_as::<_, (Uuid, i32)>(
    "SELECT variant_id, quantity FROM order_items WHERE order_id = $1",
  )
  .bind(order_id)
  .fetch_all(&mut **tx)
  .await
}

async fn fallback_from_order_items(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  convert: bool,
) -> Result<(), sqlx::Error> {
  let items = order_item_quantities(tx, order_id).await?;
  let variant_ids: Vec<Uuid> = items.iter().map(|(id, _)| *id).collect();
  let mut rows = lock_variant_inventory(tx, &variant_ids).await?;

  for (variant_id, quantity) in items {
    let mut remaining = quantity;
    for row in rows.iter_mut().filter(|row| row.variant_id == variant_id) {
      if remaining <= 0 {
        break;
      }
      let take = remaining.min(row.reserved.max(0));
      let take = if convert {
        take.min(row.quantity.max(0))
      } else {
        take
      };
      if take <= 0 {
        continue;
      }
      if convert {
        sqlx::query(
          "UPDATE inventory
           SET reserved = GREATEST(reserved - $2, 0),
               quantity = GREATEST(quantity - $2, 0),
               updated_at = now()
           WHERE id = $1",
        )
        .bind(row.id)
        .bind(take)
        .execute(&mut **tx)
        .await?;
        row.quantity -= take;
      } else {
        sqlx::query(
          "UPDATE inventory
           SET reserved = GREATEST(reserved - $2, 0), updated_at = now()
           WHERE id = $1",
        )
        .bind(row.id)
        .bind(take)
        .execute(&mut **tx)
        .await?;
      }
      row.reserved -= take;
      remaining -= take;
    }
  }
  Ok(())
}

/// Restock sold units onto the default (or first) warehouse after convert.
pub async fn restock_sold_items(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<(), sqlx::Error> {
  let items = order_item_quantities(tx, order_id).await?;
  let variant_ids: Vec<Uuid> = items
    .iter()
    .filter(|(_, quantity)| *quantity > 0)
    .map(|(id, _)| *id)
    .collect();
  let rows = lock_variant_inventory(tx, &variant_ids).await?;

  for (variant_id, quantity) in items {
    if quantity <= 0 {
      continue;
    }
    let of_variant = |row: &&InventoryLockRow| row.variant_id == variant_id;
    let Some(target) = rows
      .iter()
      .filter(of_variant)
      .find(|row| row.is_default)
      .or_else(|| rows.iter().find(of_variant))
    else {
      tracing::warn!(
        %order_id,
        %variant_id,
        quantity,
        "restock skipped: variant has no active-warehouse inventory row"
      );
      continue;
    };
    sqlx::query(
      "UPDATE inventory SET quantity = quantity + $2, updated_at = now() WHERE id = $1",
    )
    .bind(target.id)
    .bind(quantity)
    .execute(&mut **tx)
    .await?;
  }
  Ok(())
}

/// Adjust on-hand quantity by `delta` (can be negative). Rejects CHECK breaks.
/// Returns the warehouse id of the adjusted row (for audit metadata).
pub async fn adjust_stock(
  tx: &mut Transaction<'_, Postgres>,
  inventory_id: Uuid,
  delta: i32,
) -> Result<Uuid, InventoryError> {
  // Single row, single statement: trivially within the lock order.
  let row = sqlx::query_as::<_, (i32, i32, Uuid)>(
    "SELECT quantity, reserved, warehouse_id FROM inventory WHERE id = $1 FOR UPDATE",
  )
  .bind(inventory_id)
  .fetch_optional(&mut **tx)
  .await?;
  let Some((quantity, reserved, warehouse_id)) = row else {
    return Err(InventoryError::Insufficient);
  };
  let next = quantity
    .checked_add(delta)
    .ok_or(InventoryError::Insufficient)?;
  if next < 0 || reserved > next {
    return Err(InventoryError::Insufficient);
  }
  sqlx::query(
    "UPDATE inventory SET quantity = $2, updated_at = now() WHERE id = $1",
  )
  .bind(inventory_id)
  .bind(next)
  .execute(&mut **tx)
  .await?;
  Ok(warehouse_id)
}

#[cfg(test)]
mod tests {
  use super::*;
  use sqlx::PgPool;

  async fn seed_order_item_without_reservation(
    pool: &PgPool,
    quantity: i32,
    reserved: i32,
    item_qty: i32,
  ) -> (Uuid, Uuid) {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let warehouse_id = Uuid::now_v7();
    let inventory_id = Uuid::now_v7();
    let order_id = Uuid::now_v7();

    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'T', $2, 1000)",
    )
    .bind(product_id)
    .bind(format!("p-{product_id}"))
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
       VALUES ($1, $2, 'W', true, true)",
    )
    .bind(warehouse_id)
    .bind(format!("wh-{}", warehouse_id.simple()))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
       VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(inventory_id)
    .bind(variant_id)
    .bind(warehouse_id)
    .bind(quantity)
    .bind(reserved)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO orders (id, guest_email, status, access_token, total_cents)
       VALUES ($1, 'a@b.co', 'pending_payment', $2, 1000)",
    )
    .bind(order_id)
    .bind(Uuid::now_v7())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
       VALUES ($1, $2, $3, 'T', 'M / azul', $4, 1000)",
    )
    .bind(Uuid::now_v7())
    .bind(order_id)
    .bind(variant_id)
    .bind(item_qty)
    .execute(pool)
    .await
    .unwrap();
    (order_id, inventory_id)
  }

  #[tokio::test]
  async fn fallback_release_twice_does_not_go_negative() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, inventory_id) =
      seed_order_item_without_reservation(&pool, 5, 1, 1).await;
    let mut tx = pool.begin().await.unwrap();
    release_reservation(&mut tx, order_id).await.unwrap();
    release_reservation(&mut tx, order_id).await.unwrap();
    let (quantity, reserved): (i32, i32) =
      sqlx::query_as("SELECT quantity, reserved FROM inventory WHERE id = $1")
        .bind(inventory_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(quantity, 5);
    assert_eq!(reserved, 0);
  }

  #[tokio::test]
  async fn fallback_convert_twice_does_not_go_negative() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, inventory_id) =
      seed_order_item_without_reservation(&pool, 5, 1, 1).await;
    let mut tx = pool.begin().await.unwrap();
    convert_reservation(&mut tx, order_id).await.unwrap();
    convert_reservation(&mut tx, order_id).await.unwrap();
    let (quantity, reserved): (i32, i32) =
      sqlx::query_as("SELECT quantity, reserved FROM inventory WHERE id = $1")
        .bind(inventory_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(quantity, 4);
    assert_eq!(reserved, 0);
  }

  /// Product + one variant with a default and a secondary active warehouse
  /// row of `per_row` units each. Returns the variant id.
  async fn seed_variant_two_rows(pool: &PgPool, per_row: i32) -> Uuid {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents, active)
       VALUES ($1, 'T', $2, 1000, true)",
    )
    .bind(product_id)
    .bind(format!("p-{product_id}"))
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
    for is_default in [true, false] {
      let warehouse_id = Uuid::now_v7();
      sqlx::query(
        "INSERT INTO warehouses (id, code, name, is_default, active)
         VALUES ($1, $2, 'W', $3, true)",
      )
      .bind(warehouse_id)
      .bind(format!("wh-{}", warehouse_id.simple()))
      .bind(is_default)
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
      .bind(per_row)
      .execute(pool)
      .await
      .unwrap();
    }
    variant_id
  }

  /// Pending order over `lines` (no reservation yet).
  async fn seed_order(pool: &PgPool, lines: &[(Uuid, i32)]) -> Uuid {
    let order_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO orders (id, guest_email, status, access_token, total_cents)
       VALUES ($1, 'a@b.co', 'pending_payment', $2, 1000)",
    )
    .bind(order_id)
    .bind(Uuid::now_v7())
    .execute(pool)
    .await
    .unwrap();
    for (variant_id, quantity) in lines {
      sqlx::query(
        "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
         VALUES ($1, $2, $3, 'T', 'M / azul', $4, 1000)",
      )
      .bind(Uuid::now_v7())
      .bind(order_id)
      .bind(variant_id)
      .bind(quantity)
      .execute(pool)
      .await
      .unwrap();
    }
    order_id
  }

  fn reserve_lines(lines: &[(Uuid, i32)]) -> Vec<ReserveLine> {
    lines
      .iter()
      .map(|(variant_id, quantity)| ReserveLine {
        variant_id: *variant_id,
        quantity: *quantity,
      })
      .collect()
  }

  async fn seed_reserved_order(pool: &PgPool, lines: &[(Uuid, i32)]) -> Uuid {
    let order_id = seed_order(pool, lines).await;
    let mut tx = pool.begin().await.unwrap();
    reserve_for_order(&mut tx, order_id, &reserve_lines(lines))
      .await
      .unwrap();
    tx.commit().await.unwrap();
    order_id
  }

  async fn expire(pool: &PgPool, order_id: Uuid) {
    sqlx::query(
      "UPDATE orders SET reservation_expires_at = now() - interval '1 minute'
       WHERE id = $1",
    )
    .bind(order_id)
    .execute(pool)
    .await
    .unwrap();
  }

  async fn seed_cart(pool: &PgPool, lines: &[(Uuid, i32)]) -> Uuid {
    let cart_id = Uuid::now_v7();
    sqlx::query("INSERT INTO carts (id, token) VALUES ($1, $2)")
      .bind(cart_id)
      .bind(Uuid::now_v7())
      .execute(pool)
      .await
      .unwrap();
    for (variant_id, quantity) in lines {
      sqlx::query(
        "INSERT INTO cart_items (id, cart_id, variant_id, quantity)
         VALUES ($1, $2, $3, $4)",
      )
      .bind(Uuid::now_v7())
      .bind(cart_id)
      .bind(variant_id)
      .bind(quantity)
      .execute(pool)
      .await
      .unwrap();
    }
    cart_id
  }

  /// Checkout's stock steps: lock the cart's rows (reclaiming expired
  /// holds), then reserve for a new order.
  async fn checkout_style(
    pool: &PgPool,
    cart_id: Uuid,
    lines: &[(Uuid, i32)],
  ) -> Result<Uuid, String> {
    let order_id = seed_order(pool, lines).await;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    lock_cart_inventory(&mut tx, cart_id)
      .await
      .map_err(|e| e.to_string())?;
    reserve_for_order(&mut tx, order_id, &reserve_lines(lines))
      .await
      .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(order_id)
  }

  async fn live_available(pool: &PgPool, variant_id: Uuid) -> i64 {
    sqlx::query_scalar(&format!(
      "SELECT COALESCE(SUM({LIVE_AVAILABLE_SQL}), 0)::bigint
       FROM inventory i WHERE i.variant_id = $1"
    ))
    .bind(variant_id)
    .fetch_one(pool)
    .await
    .unwrap()
  }

  async fn status_of(pool: &PgPool, order_id: Uuid) -> String {
    sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
      .bind(order_id)
      .fetch_one(pool)
      .await
      .unwrap()
  }

  #[tokio::test]
  async fn expired_unswept_hold_does_not_block_availability() {
    // Reclaims expired holds: needs a database of its own.
    let pool = crate::test_support::isolated_pool().await;
    let variant_id = seed_variant_two_rows(&pool, 1).await;
    let lines = [(variant_id, 2)];
    let stale = seed_reserved_order(&pool, &lines).await;
    assert_eq!(live_available(&pool, variant_id).await, 0);

    expire(&pool, stale).await;
    // Not swept yet: the raw counter still holds both units...
    let raw: i64 = sqlx::query_scalar(
      "SELECT SUM(quantity - reserved)::bigint FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw, 0);
    // ...but display and the cart treat them as free.
    assert_eq!(live_available(&pool, variant_id).await, 2);
    let cart_id = seed_cart(&pool, &[]).await;
    crate::app::store::cart::add_item(&pool, cart_id, variant_id, 2)
      .await
      .expect("expired hold must not block add to cart");

    // Checkout reclaims the stale hold (cancelling it) and reserves both.
    let fresh = checkout_style(&pool, cart_id, &lines)
      .await
      .expect("checkout reclaims expired hold");
    assert_eq!(status_of(&pool, stale).await, "cancelled");
    assert_eq!(status_of(&pool, fresh).await, "pending_payment");
    let held: i64 = sqlx::query_scalar(
      "SELECT COALESCE(SUM(quantity), 0)::bigint
       FROM inventory_reservations WHERE order_id = $1",
    )
    .bind(fresh)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(held, 2);
    assert_eq!(live_available(&pool, variant_id).await, 0);
  }

  #[derive(Clone, Copy)]
  enum Op {
    Pay,
    Cancel,
    PayRefund,
    Sweep,
    Checkout,
  }

  async fn run_event(
    pool: &PgPool,
    order_id: Uuid,
    event: OrderEvent,
  ) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    apply(&mut tx, order_id, event, ApplyOptions::default())
      .await
      .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())
  }

  async fn run_op(
    pool: PgPool,
    op: Op,
    target: Uuid,
    lines: Vec<(Uuid, i32)>,
  ) -> Result<Option<Uuid>, String> {
    match op {
      Op::Pay => run_event(&pool, target, OrderEvent::Pay)
        .await
        .map(|_| None),
      Op::Cancel => run_event(&pool, target, OrderEvent::Cancel)
        .await
        .map(|_| None),
      Op::PayRefund => {
        run_event(&pool, target, OrderEvent::Pay).await?;
        run_event(&pool, target, OrderEvent::Refund)
          .await
          .map(|_| None)
      }
      Op::Sweep => crate::app::store::order::expire_pending_reservations(&pool)
        .await
        .map(|_| None)
        .map_err(|e| e.to_string()),
      Op::Checkout => checkout_style(&pool, target, &lines).await.map(Some),
    }
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn concurrent_pay_cancel_checkout_do_not_deadlock() {
    // Runs the global sweep: needs a database of its own.
    let pool = crate::test_support::isolated_pool().await;
    const PER_ROW: i32 = 500;
    let mut variants = Vec::new();
    for _ in 0..4 {
      variants.push(seed_variant_two_rows(&pool, PER_ROW).await);
    }
    // Overlapping lines in rotating (and alternately reversed) variant
    // order: per-line locking would take rows in conflicting orders.
    let lines_for = |k: usize| -> Vec<(Uuid, i32)> {
      let mut lines: Vec<(Uuid, i32)> = (0..3)
        .map(|j| (variants[(k + j) % variants.len()], 1 + j as i32))
        .collect();
      if k % 2 == 1 {
        lines.reverse();
      }
      lines
    };

    let mut jobs = Vec::new();
    for k in 0..40 {
      let lines = lines_for(k);
      let op = match k % 5 {
        0 => Op::Pay,
        1 => Op::Cancel,
        2 => Op::PayRefund,
        3 => Op::Sweep,
        _ => Op::Checkout,
      };
      let target = match op {
        Op::Checkout => seed_cart(&pool, &lines).await,
        _ => {
          let order_id = seed_reserved_order(&pool, &lines).await;
          if matches!(op, Op::Sweep) {
            // Its hold is expired: the sweeps and the checkouts' reclaim
            // race to cancel it.
            expire(&pool, order_id).await;
          }
          order_id
        }
      };
      jobs.push((op, target, lines));
    }

    let handles: Vec<_> = jobs
      .iter()
      .map(|(op, target, lines)| {
        tokio::spawn(run_op(pool.clone(), *op, *target, lines.clone()))
      })
      .collect();
    let mut checkout_orders = 0;
    for handle in handles {
      match handle.await.expect("task panicked") {
        Ok(Some(_)) => checkout_orders += 1,
        Ok(None) => {}
        Err(error) => {
          assert!(
            !error.contains("deadlock"),
            "deadlock (40P01) under concurrent stock ops: {error}"
          );
          panic!("stock op failed: {error}");
        }
      }
    }
    // Settle any expired order a racing sweep skipped while it was locked.
    crate::app::store::order::expire_pending_reservations(&pool)
      .await
      .unwrap();

    // Expected per variant: sold = units of orders left paid; held = units
    // of the checkout-created orders, still pending.
    let mut sold: HashMap<Uuid, i32> = HashMap::new();
    let mut held: HashMap<Uuid, i32> = HashMap::new();
    for (op, target, lines) in &jobs {
      if !matches!(op, Op::Checkout) {
        let expected = match op {
          Op::Pay => "paid",
          Op::PayRefund => "refunded",
          _ => "cancelled",
        };
        assert_eq!(status_of(&pool, *target).await, expected);
      }
      for (variant_id, quantity) in lines {
        match op {
          Op::Pay => *sold.entry(*variant_id).or_default() += quantity,
          Op::Checkout => *held.entry(*variant_id).or_default() += quantity,
          _ => {}
        }
      }
    }
    assert_eq!(
      checkout_orders,
      jobs
        .iter()
        .filter(|(op, _, _)| matches!(op, Op::Checkout))
        .count()
    );
    for variant_id in &variants {
      let (quantity, reserved): (i64, i64) = sqlx::query_as(
        "SELECT SUM(quantity)::bigint, SUM(reserved)::bigint
         FROM inventory WHERE variant_id = $1",
      )
      .bind(variant_id)
      .fetch_one(&pool)
      .await
      .unwrap();
      let sold = sold.get(variant_id).copied().unwrap_or(0);
      let held = held.get(variant_id).copied().unwrap_or(0);
      assert_eq!(quantity, i64::from(2 * PER_ROW - sold), "on hand");
      assert_eq!(reserved, i64::from(held), "reserved");
    }
    // Every row's counter matches its reservation rows.
    let drift: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM inventory i
       WHERE i.reserved <> COALESCE((
         SELECT SUM(r.quantity) FROM inventory_reservations r
         WHERE r.inventory_id = i.id), 0)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(drift, 0);
  }
}
