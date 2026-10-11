use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::coupons::restore_redemption_for_order;
use super::inventory::{
  convert_reservation, release_reservation, restock_sold_items,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
  PendingPayment,
  Paid,
  Processing,
  Shipped,
  Delivered,
  Cancelled,
  Refunded,
}

impl OrderStatus {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw {
      "pending_payment" => Some(Self::PendingPayment),
      "paid" => Some(Self::Paid),
      "processing" => Some(Self::Processing),
      "shipped" => Some(Self::Shipped),
      "delivered" => Some(Self::Delivered),
      "cancelled" => Some(Self::Cancelled),
      "refunded" => Some(Self::Refunded),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::PendingPayment => "pending_payment",
      Self::Paid => "paid",
      Self::Processing => "processing",
      Self::Shipped => "shipped",
      Self::Delivered => "delivered",
      Self::Cancelled => "cancelled",
      Self::Refunded => "refunded",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderEvent {
  Pay,
  Ship,
  Deliver,
  Cancel,
  Refund,
}

impl OrderEvent {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw {
      "paid" | "pay" => Some(Self::Pay),
      "shipped" | "ship" => Some(Self::Ship),
      "delivered" | "deliver" => Some(Self::Deliver),
      "cancelled" | "cancel" => Some(Self::Cancel),
      "refunded" | "refund" => Some(Self::Refund),
      _ => None,
    }
  }
}

#[derive(Debug)]
pub enum ApplyError {
  NotFound,
  Illegal {
    from: OrderStatus,
    event: OrderEvent,
  },
  Db(sqlx::Error),
}

impl From<sqlx::Error> for ApplyError {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

impl std::fmt::Display for ApplyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::NotFound => write!(f, "Pedido não encontrado."),
      Self::Illegal { from, event } => {
        write!(f, "Transição inválida: {} + {:?}.", from.as_str(), event)
      }
      Self::Db(error) => write!(f, "{error}"),
    }
  }
}

fn transition(
  from: OrderStatus,
  event: OrderEvent,
) -> Result<OrderStatus, ApplyError> {
  let next = match (from, event) {
    (OrderStatus::PendingPayment, OrderEvent::Pay) => OrderStatus::Paid,
    (OrderStatus::PendingPayment, OrderEvent::Cancel) => OrderStatus::Cancelled,
    (OrderStatus::Paid, OrderEvent::Ship) => OrderStatus::Shipped,
    (OrderStatus::Shipped, OrderEvent::Deliver) => OrderStatus::Delivered,
    (OrderStatus::Paid | OrderStatus::Shipped, OrderEvent::Cancel) => {
      OrderStatus::Cancelled
    }
    (OrderStatus::Paid | OrderStatus::Shipped, OrderEvent::Refund) => {
      OrderStatus::Refunded
    }
    _ => {
      return Err(ApplyError::Illegal { from, event });
    }
  };
  Ok(next)
}

/// Apply one order event under `SELECT … FOR UPDATE`. Side effects (stock,
/// coupon restore, `paid_at`) run in the same transaction.
pub async fn apply(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  event: OrderEvent,
) -> Result<OrderStatus, ApplyError> {
  let current: Option<String> = sqlx::query_scalar(
    "SELECT status::text FROM orders WHERE id = $1 FOR UPDATE",
  )
  .bind(order_id)
  .fetch_optional(&mut **tx)
  .await?;
  let Some(raw) = current else {
    return Err(ApplyError::NotFound);
  };
  let from = OrderStatus::parse(&raw).ok_or(ApplyError::NotFound)?;
  let next = transition(from, event)?;

  match (from, event) {
    (OrderStatus::PendingPayment, OrderEvent::Pay) => {
      convert_reservation(tx, order_id).await?;
      sqlx::query(
        "UPDATE orders SET status = $2::order_status, paid_at = now(), updated_at = now()
         WHERE id = $1",
      )
      .bind(order_id)
      .bind(next.as_str())
      .execute(&mut **tx)
      .await?;
    }
    (OrderStatus::PendingPayment, OrderEvent::Cancel) => {
      release_reservation(tx, order_id).await?;
      restore_redemption_for_order(tx, order_id).await?;
      sqlx::query(
        "UPDATE orders SET status = $2::order_status, updated_at = now() WHERE id = $1",
      )
      .bind(order_id)
      .bind(next.as_str())
      .execute(&mut **tx)
      .await?;
    }
    (_, OrderEvent::Cancel | OrderEvent::Refund) => {
      restock_sold_items(tx, order_id).await?;
      restore_redemption_for_order(tx, order_id).await?;
      sqlx::query(
        "UPDATE orders SET status = $2::order_status, updated_at = now() WHERE id = $1",
      )
      .bind(order_id)
      .bind(next.as_str())
      .execute(&mut **tx)
      .await?;
    }
    _ => {
      sqlx::query(
        "UPDATE orders SET status = $2::order_status, updated_at = now() WHERE id = $1",
      )
      .bind(order_id)
      .bind(next.as_str())
      .execute(&mut **tx)
      .await?;
    }
  }

  Ok(next)
}

pub async fn expire_pending_reservations(
  pool: &PgPool,
) -> Result<(), ApplyError> {
  let mut tx = pool.begin().await?;
  match expire_pending_reservations_in_tx(&mut tx).await {
    Ok(()) => {
      tx.commit().await?;
      Ok(())
    }
    Err(error) => {
      let _ = tx.rollback().await;
      Err(error)
    }
  }
}

pub async fn expire_pending_reservations_in_tx(
  tx: &mut Transaction<'_, Postgres>,
) -> Result<(), ApplyError> {
  let expired: Vec<Uuid> = sqlx::query_scalar(
    r#"
        SELECT id FROM orders
        WHERE status = 'pending_payment'
          AND reservation_expires_at IS NOT NULL
          AND reservation_expires_at < now()
        FOR UPDATE SKIP LOCKED
        "#,
  )
  .fetch_all(&mut **tx)
  .await?;

  for order_id in expired {
    apply(tx, order_id, OrderEvent::Cancel).await?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::store::inventory::ReserveLine;
  use crate::app::store::inventory::reserve_for_order;

  async fn seed_order_with_stock(pool: &PgPool, qty: i32) -> (Uuid, Uuid, i32) {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let warehouse_id = Uuid::now_v7();
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
       VALUES ($1, $2, $3, $4, 0)",
    )
    .bind(Uuid::now_v7())
    .bind(variant_id)
    .bind(warehouse_id)
    .bind(qty)
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
       VALUES ($1, $2, $3, 'T', 'M / azul', 1, 1000)",
    )
    .bind(Uuid::now_v7())
    .bind(order_id)
    .bind(variant_id)
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
    (order_id, variant_id, qty)
  }

  #[tokio::test]
  async fn apply_rejects_illegal_transition() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 2).await;
    let mut tx = pool.begin().await.unwrap();
    apply(&mut tx, order_id, OrderEvent::Pay).await.unwrap();
    apply(&mut tx, order_id, OrderEvent::Deliver)
      .await
      .unwrap_err();
    apply(&mut tx, order_id, OrderEvent::Pay).await.unwrap_err();
    tx.rollback().await.unwrap();
  }

  #[tokio::test]
  async fn paid_shipped_delivered() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 2).await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Pay).await.unwrap(),
      OrderStatus::Paid
    );
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Ship).await.unwrap(),
      OrderStatus::Shipped
    );
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Deliver).await.unwrap(),
      OrderStatus::Delivered
    );
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn paid_refund_restocks() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    apply(&mut tx, order_id, OrderEvent::Pay).await.unwrap();
    let after_pay: i32 = sqlx::query_scalar(
      "SELECT quantity FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(after_pay, qty - 1);
    apply(&mut tx, order_id, OrderEvent::Refund).await.unwrap();
    let after: i32 = sqlx::query_scalar(
      "SELECT quantity FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(after, qty);
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn revenue_sums_paid_through_delivered_only() {
    let pool = crate::test_support::fresh_pool().await;
    let mut ids = Vec::new();
    for (status, cents) in [
      ("pending_payment", 99_999_i64),
      ("cancelled", 50_000),
      ("refunded", 20_000),
      ("paid", 1_000),
      ("shipped", 300),
      ("delivered", 400),
    ] {
      let id = Uuid::now_v7();
      sqlx::query(
        "INSERT INTO orders (id, guest_email, status, access_token, total_cents)
         VALUES ($1, 'a@b.co', $2::order_status, $3, $4)",
      )
      .bind(id)
      .bind(status)
      .bind(Uuid::now_v7())
      .bind(cents)
      .execute(&pool)
      .await
      .unwrap();
      ids.push(id);
    }
    let revenue: i64 = sqlx::query_scalar(
      "SELECT COALESCE(SUM(total_cents), 0)::bigint FROM orders
       WHERE id = ANY($1) AND status IN ('paid', 'shipped', 'delivered')",
    )
    .bind(&ids)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revenue, 1_700);
  }
}
