use sqlx::{Postgres, Transaction};
use uuid::Uuid;

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
  quantity: i32,
  reserved: i32,
  is_default: bool,
}

#[derive(sqlx::FromRow)]
struct ReservationRow {
  inventory_id: Uuid,
  quantity: i32,
}

async fn lock_variant_inventory(
  tx: &mut Transaction<'_, Postgres>,
  variant_id: Uuid,
) -> Result<Vec<InventoryLockRow>, sqlx::Error> {
  sqlx::query_as::<_, InventoryLockRow>(
    r#"
        SELECT i.id, i.quantity, i.reserved, w.is_default
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id = $1 AND w.active = true
        ORDER BY i.id
        FOR UPDATE OF i
        "#,
  )
  .bind(variant_id)
  .fetch_all(&mut **tx)
  .await
}

/// Lock every active-warehouse inventory row for the cart, in `id` order,
/// so checkout quoting and reserve share one lock order.
pub async fn lock_cart_inventory(
  tx: &mut Transaction<'_, Postgres>,
  cart_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    r#"
        SELECT i.id
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        INNER JOIN cart_items ci ON ci.variant_id = i.variant_id
        WHERE ci.cart_id = $1 AND w.active = true
        ORDER BY i.id
        FOR UPDATE OF i
        "#,
  )
  .bind(cart_id)
  .execute(&mut **tx)
  .await?;
  Ok(())
}

/// Hold stock for a pending PIX order: `reserved += qty` across warehouses
/// (default first) and insert one [`inventory_reservations`] row per take.
pub async fn reserve_for_order(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  lines: &[ReserveLine],
) -> Result<(), InventoryError> {
  for line in lines {
    if line.quantity <= 0 {
      continue;
    }
    let mut rows = lock_variant_inventory(tx, line.variant_id).await?;
    let available: i32 = rows
      .iter()
      .map(|row| (row.quantity - row.reserved).max(0))
      .sum();
    if available < line.quantity {
      return Err(InventoryError::Insufficient);
    }
    let mut remaining = line.quantity;
    rows.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.id.cmp(&b.id)));

    for row in rows {
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
  rows: &[ReservationRow],
  convert: bool,
) -> Result<(), sqlx::Error> {
  for row in rows {
    sqlx::query("SELECT id FROM inventory WHERE id = $1 FOR UPDATE")
      .bind(row.inventory_id)
      .execute(&mut **tx)
      .await?;
    if convert {
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
    apply_reservation_rows(tx, &rows, true).await?;
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
    apply_reservation_rows(tx, &rows, false).await?;
    clear_reservation_rows(tx, order_id).await?;
  }
  sqlx::query("UPDATE orders SET reservation_expires_at = NULL WHERE id = $1")
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
  Ok(())
}

async fn fallback_from_order_items(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  convert: bool,
) -> Result<(), sqlx::Error> {
  let items = sqlx::query_as::<_, (Uuid, i32)>(
    "SELECT variant_id, quantity FROM order_items WHERE order_id = $1",
  )
  .bind(order_id)
  .fetch_all(&mut **tx)
  .await?;

  for (variant_id, quantity) in items {
    let mut remaining = quantity;
    let rows = lock_variant_inventory(tx, variant_id).await?;
    for row in rows {
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
  let items = sqlx::query_as::<_, (Uuid, i32)>(
    "SELECT variant_id, quantity FROM order_items WHERE order_id = $1",
  )
  .bind(order_id)
  .fetch_all(&mut **tx)
  .await?;

  for (variant_id, quantity) in items {
    if quantity <= 0 {
      continue;
    }
    let rows = lock_variant_inventory(tx, variant_id).await?;
    let Some(target) = rows
      .iter()
      .find(|row| row.is_default)
      .or_else(|| rows.first())
    else {
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
}
