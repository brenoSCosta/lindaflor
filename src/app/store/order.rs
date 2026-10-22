use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::coupons::{
  reclaim_redemption_for_order, restore_redemption_for_order,
};
use super::inventory::{
  InventoryError, ReserveLine, convert_reservation, release_reservation,
  reserve_for_order, restock_sold_items,
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
  Process,
  Ship,
  Deliver,
  Cancel,
  Refund,
  Reopen,
  ReopenPay,
}

impl OrderEvent {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw {
      "paid" | "pay" => Some(Self::Pay),
      "processing" | "process" => Some(Self::Process),
      "shipped" | "ship" => Some(Self::Ship),
      "delivered" | "deliver" => Some(Self::Deliver),
      "cancelled" | "cancel" => Some(Self::Cancel),
      "refunded" | "refund" => Some(Self::Refund),
      "reopen" => Some(Self::Reopen),
      "reopen_pay" | "reopen_and_pay" => Some(Self::ReopenPay),
      _ => None,
    }
  }

  /// Stable name for audit metadata.
  pub fn action_name(self) -> &'static str {
    match self {
      Self::Pay => "pay",
      Self::Process => "process",
      Self::Ship => "ship",
      Self::Deliver => "deliver",
      Self::Cancel => "cancel",
      Self::Refund => "refund",
      Self::Reopen => "reopen",
      Self::ReopenPay => "reopen_pay",
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
  InsufficientStock,
  Db(sqlx::Error),
}

impl From<sqlx::Error> for ApplyError {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

impl From<InventoryError> for ApplyError {
  fn from(error: InventoryError) -> Self {
    match error {
      InventoryError::Insufficient => Self::InsufficientStock,
      InventoryError::Db(error) => Self::Db(error),
    }
  }
}

impl std::fmt::Display for ApplyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::NotFound => write!(f, "Pedido não encontrado."),
      Self::Illegal { from, event } => {
        write!(f, "Transição inválida: {} + {:?}.", from.as_str(), event)
      }
      Self::InsufficientStock => {
        write!(f, "Estoque insuficiente para reabrir o pedido.")
      }
      Self::Db(error) => write!(f, "{error}"),
    }
  }
}

/// Per-apply knobs. `restock: None` uses the status default (restock when
/// cancelling/refunding a `Paid`/`Processing` order, keep stock sold once
/// the goods left the warehouse from `Shipped`/`Delivered`); `Some` is an
/// explicit admin choice from the "Devolver ao estoque" checkbox.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplyOptions {
  pub restock: Option<bool>,
}

impl ApplyOptions {
  pub fn with_restock(restock: bool) -> Self {
    Self {
      restock: Some(restock),
    }
  }

  fn resolve_restock(self, from: OrderStatus) -> bool {
    match self.restock {
      Some(value) => value,
      None => {
        matches!(from, OrderStatus::Paid | OrderStatus::Processing)
      }
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
    (OrderStatus::Paid, OrderEvent::Process) => OrderStatus::Processing,
    (OrderStatus::Paid | OrderStatus::Processing, OrderEvent::Ship) => {
      OrderStatus::Shipped
    }
    (OrderStatus::Shipped, OrderEvent::Deliver) => OrderStatus::Delivered,
    (
      OrderStatus::Paid | OrderStatus::Processing | OrderStatus::Shipped,
      OrderEvent::Cancel,
    ) => OrderStatus::Cancelled,
    (
      OrderStatus::Paid
      | OrderStatus::Processing
      | OrderStatus::Shipped
      | OrderStatus::Delivered,
      OrderEvent::Refund,
    ) => OrderStatus::Refunded,
    (OrderStatus::Cancelled, OrderEvent::Reopen) => OrderStatus::PendingPayment,
    (OrderStatus::Cancelled, OrderEvent::ReopenPay) => OrderStatus::Paid,
    _ => {
      return Err(ApplyError::Illegal { from, event });
    }
  };
  Ok(next)
}

struct ApplyOutcome {
  from: OrderStatus,
  next: OrderStatus,
  restocked: bool,
}

async fn order_lines(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<Vec<ReserveLine>, sqlx::Error> {
  let items = sqlx::query_as::<_, (Uuid, i32)>(
    "SELECT variant_id, quantity FROM order_items WHERE order_id = $1",
  )
  .bind(order_id)
  .fetch_all(&mut **tx)
  .await?;
  Ok(
    items
      .into_iter()
      .map(|(variant_id, quantity)| ReserveLine {
        variant_id,
        quantity,
      })
      .collect(),
  )
}

/// Apply one order event under `SELECT … FOR UPDATE`. Side effects (stock,
/// coupon restore, `paid_at`) run in the same transaction.
pub async fn apply(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  event: OrderEvent,
  opts: ApplyOptions,
) -> Result<OrderStatus, ApplyError> {
  Ok(apply_inner(tx, order_id, event, opts).await?.next)
}

async fn apply_inner(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  event: OrderEvent,
  opts: ApplyOptions,
) -> Result<ApplyOutcome, ApplyError> {
  let row: Option<(String, Option<String>)> = sqlx::query_as(
    "SELECT status::text, paid_at::text FROM orders WHERE id = $1 FOR UPDATE",
  )
  .bind(order_id)
  .fetch_optional(&mut **tx)
  .await?;
  let Some((raw, paid_at)) = row else {
    return Err(ApplyError::NotFound);
  };
  let from = OrderStatus::parse(&raw).ok_or(ApplyError::NotFound)?;
  // Reopen is admin-only recovery for orders cancelled while still
  // `pending_payment` and never paid. Anything paid (or never cancelled)
  // stays on the normal edges.
  if matches!(event, OrderEvent::Reopen | OrderEvent::ReopenPay)
    && (from != OrderStatus::Cancelled || paid_at.is_some())
  {
    return Err(ApplyError::Illegal { from, event });
  }
  let next = transition(from, event)?;
  let mut restocked = false;

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
    (OrderStatus::Cancelled, OrderEvent::Reopen) => {
      let lines = order_lines(tx, order_id).await?;
      reserve_for_order(tx, order_id, &lines).await?;
      reclaim_redemption_for_order(tx, order_id).await?;
      sqlx::query(
        "UPDATE orders SET status = $2::order_status, updated_at = now() WHERE id = $1",
      )
      .bind(order_id)
      .bind(next.as_str())
      .execute(&mut **tx)
      .await?;
    }
    (OrderStatus::Cancelled, OrderEvent::ReopenPay) => {
      let lines = order_lines(tx, order_id).await?;
      reserve_for_order(tx, order_id, &lines).await?;
      convert_reservation(tx, order_id).await?;
      reclaim_redemption_for_order(tx, order_id).await?;
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
      restocked = opts.resolve_restock(from);
      if restocked {
        restock_sold_items(tx, order_id).await?;
      }
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

  Ok(ApplyOutcome {
    from,
    next,
    restocked,
  })
}

/// Admin apply plus the `audit_events` row, in the same transaction.
/// The admin page must use this instead of [`apply`] so every status
/// change leaves a trail.
pub async fn apply_with_audit(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  event: OrderEvent,
  opts: ApplyOptions,
  actor_id: Option<Uuid>,
  actor_name: Option<&str>,
) -> Result<OrderStatus, ApplyError> {
  let outcome = apply_inner(tx, order_id, event, opts).await?;
  let aggregate_id = order_id.to_string();
  crate::audit::record_audit_event(
    &mut **tx,
    crate::audit::AuditEvent {
      aggregate_type: "order",
      aggregate_id: aggregate_id.as_str(),
      entity_type: "order",
      entity_id: order_id,
      actor_id,
      actor_name,
      metadata: serde_json::json!({
        "action": event.action_name(),
        "from": outcome.from.as_str(),
        "to": outcome.next.as_str(),
        "restock": outcome.restocked,
      }),
    },
  )
  .await?;
  Ok(outcome.next)
}

/// Background sweep (see `main.rs`): cancel every pending order whose hold
/// has expired, one transaction per order. A single transaction cancelling
/// several orders would lock each order's inventory rows in turn, which
/// breaks the ascending-`id` lock order (see `inventory.rs`). Orders another
/// transaction holds are skipped and picked up on the next run.
pub async fn expire_pending_reservations(
  pool: &PgPool,
) -> Result<(), ApplyError> {
  let expired: Vec<Uuid> = sqlx::query_scalar(
    r#"
        SELECT id FROM orders
        WHERE status = 'pending_payment'
          AND reservation_expires_at IS NOT NULL
          AND reservation_expires_at < now()
        ORDER BY id
        "#,
  )
  .fetch_all(pool)
  .await?;

  let mut first_error = None;
  for order_id in expired {
    if let Err(error) = expire_one(pool, order_id).await {
      tracing::error!(%order_id, %error, "reservation expiry failed");
      first_error.get_or_insert(error);
    }
  }
  first_error.map_or(Ok(()), Err)
}

/// Cancel one expired pending order (no-op unless it is still pending and
/// past its hold). Also used by the payment-aware sweep in `src/payments`.
pub async fn expire_one(
  pool: &PgPool,
  order_id: Uuid,
) -> Result<(), ApplyError> {
  let mut tx = pool.begin().await?;
  // Re-check under the lock: the order may have been paid, cancelled or
  // reclaimed by a checkout since the scan.
  let claimed: Option<Uuid> = sqlx::query_scalar(
    r#"
        SELECT id FROM orders
        WHERE id = $1
          AND status = 'pending_payment'
          AND reservation_expires_at < now()
        FOR UPDATE SKIP LOCKED
        "#,
  )
  .bind(order_id)
  .fetch_optional(&mut *tx)
  .await?;
  if claimed.is_none() {
    return Ok(());
  }
  match apply(
    &mut tx,
    order_id,
    OrderEvent::Cancel,
    ApplyOptions::default(),
  )
  .await
  {
    Ok(_) => {
      tx.commit().await?;
      Ok(())
    }
    Err(error) => {
      let _ = tx.rollback().await;
      Err(error)
    }
  }
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
    apply(&mut tx, order_id, OrderEvent::Pay, ApplyOptions::default())
      .await
      .unwrap();
    apply(
      &mut tx,
      order_id,
      OrderEvent::Deliver,
      ApplyOptions::default(),
    )
    .await
    .unwrap_err();
    apply(&mut tx, order_id, OrderEvent::Pay, ApplyOptions::default())
      .await
      .unwrap_err();
    tx.rollback().await.unwrap();
  }

  #[tokio::test]
  async fn paid_shipped_delivered() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 2).await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Pay, ApplyOptions::default())
        .await
        .unwrap(),
      OrderStatus::Paid
    );
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Ship, ApplyOptions::default())
        .await
        .unwrap(),
      OrderStatus::Shipped
    );
    assert_eq!(
      apply(
        &mut tx,
        order_id,
        OrderEvent::Deliver,
        ApplyOptions::default()
      )
      .await
      .unwrap(),
      OrderStatus::Delivered
    );
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn paid_processing_shipped_delivered() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 2).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Pay, opts)
      .await
      .unwrap();
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Process, opts)
        .await
        .unwrap(),
      OrderStatus::Processing
    );
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Ship, opts)
        .await
        .unwrap(),
      OrderStatus::Shipped
    );
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Deliver, opts)
        .await
        .unwrap(),
      OrderStatus::Delivered
    );
    tx.commit().await.unwrap();
  }

  #[test]
  fn parse_covers_new_events() {
    assert_eq!(OrderEvent::parse("processing"), Some(OrderEvent::Process));
    assert_eq!(OrderEvent::parse("process"), Some(OrderEvent::Process));
    assert_eq!(OrderEvent::parse("reopen"), Some(OrderEvent::Reopen));
    assert_eq!(OrderEvent::parse("reopen_pay"), Some(OrderEvent::ReopenPay));
    assert_eq!(OrderEvent::parse("ship"), Some(OrderEvent::Ship));
    assert_eq!(OrderEvent::parse("unknown"), None);
  }

  async fn stock_levels(
    tx: &mut Transaction<'_, Postgres>,
    variant_id: Uuid,
  ) -> (i32, i32) {
    sqlx::query_as(
      "SELECT quantity, reserved FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&mut **tx)
    .await
    .unwrap()
  }

  async fn order_status_of(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
  ) -> String {
    sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
      .bind(order_id)
      .fetch_one(&mut **tx)
      .await
      .unwrap()
  }

  #[tokio::test]
  async fn paid_refund_restocks() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Pay, opts)
      .await
      .unwrap();
    let (after_pay, _) = stock_levels(&mut tx, variant_id).await;
    assert_eq!(after_pay, qty - 1);
    apply(&mut tx, order_id, OrderEvent::Refund, opts)
      .await
      .unwrap();
    let (after, _) = stock_levels(&mut tx, variant_id).await;
    assert_eq!(after, qty);
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn restock_flag_is_explicit() {
    let pool = crate::test_support::fresh_pool().await;
    // Paid refund with an explicit "don't restock" keeps the sale consumed.
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    apply(&mut tx, order_id, OrderEvent::Pay, ApplyOptions::default())
      .await
      .unwrap();
    apply(
      &mut tx,
      order_id,
      OrderEvent::Refund,
      ApplyOptions::with_restock(false),
    )
    .await
    .unwrap();
    let (after, _) = stock_levels(&mut tx, variant_id).await;
    assert_eq!(after, qty - 1);
    assert_eq!(order_status_of(&mut tx, order_id).await, "refunded");
    tx.commit().await.unwrap();

    // Shipped refund defaults to NOT restocking (goods are in transit).
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Pay, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Ship, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Refund, opts)
      .await
      .unwrap();
    let (after, _) = stock_levels(&mut tx, variant_id).await;
    assert_eq!(after, qty - 1);
    tx.commit().await.unwrap();

    // ...unless the admin explicitly asks to restock.
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Pay, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Ship, opts)
      .await
      .unwrap();
    apply(
      &mut tx,
      order_id,
      OrderEvent::Refund,
      ApplyOptions::with_restock(true),
    )
    .await
    .unwrap();
    let (after, _) = stock_levels(&mut tx, variant_id).await;
    assert_eq!(after, qty);
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn processing_cancel_restocks_by_default() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Pay, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Process, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Cancel, opts)
      .await
      .unwrap();
    assert_eq!(order_status_of(&mut tx, order_id).await, "cancelled");
    let (after, reserved) = stock_levels(&mut tx, variant_id).await;
    assert_eq!((after, reserved), (qty, 0));
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn delivered_can_be_refunded_without_restock() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Pay, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Ship, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Deliver, opts)
      .await
      .unwrap();
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Refund, opts)
        .await
        .unwrap(),
      OrderStatus::Refunded
    );
    let (after, _) = stock_levels(&mut tx, variant_id).await;
    assert_eq!(after, qty - 1);
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn reopen_expired_order_then_pay() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    // Expiry sweep cancels the unpaid order and releases the hold.
    apply(&mut tx, order_id, OrderEvent::Cancel, opts)
      .await
      .unwrap();
    assert_eq!(order_status_of(&mut tx, order_id).await, "cancelled");
    let (after_cancel, reserved) = stock_levels(&mut tx, variant_id).await;
    assert_eq!((after_cancel, reserved), (qty, 0));
    // Late PIX: the admin reopens, stock is re-reserved...
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Reopen, opts)
        .await
        .unwrap(),
      OrderStatus::PendingPayment
    );
    let (_, reserved) = stock_levels(&mut tx, variant_id).await;
    assert_eq!(reserved, 1);
    // ...and then records the payment as usual.
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Pay, opts)
        .await
        .unwrap(),
      OrderStatus::Paid
    );
    let (after_pay, reserved) = stock_levels(&mut tx, variant_id).await;
    assert_eq!((after_pay, reserved), (qty - 1, 0));
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn reopen_pay_marks_paid_in_one_step() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, variant_id, qty) = seed_order_with_stock(&pool, 3).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Cancel, opts)
      .await
      .unwrap();
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::ReopenPay, opts)
        .await
        .unwrap(),
      OrderStatus::Paid
    );
    let (after, reserved) = stock_levels(&mut tx, variant_id).await;
    assert_eq!((after, reserved), (qty - 1, 0));
    let paid_at: Option<String> =
      sqlx::query_scalar("SELECT paid_at::text FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(paid_at.is_some());
    // The paid order continues down the normal pipeline.
    assert_eq!(
      apply(&mut tx, order_id, OrderEvent::Ship, opts)
        .await
        .unwrap(),
      OrderStatus::Shipped
    );
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn reopen_fails_when_stock_gone() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 1).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Cancel, opts)
      .await
      .unwrap();
    // Someone else bought the last unit while the order was cancelled.
    let order_variant: Uuid = sqlx::query_scalar(
      "SELECT variant_id FROM order_items WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE inventory SET quantity = 0 WHERE variant_id = $1")
      .bind(order_variant)
      .execute(&mut *tx)
      .await
      .unwrap();
    assert!(matches!(
      apply(&mut tx, order_id, OrderEvent::Reopen, opts).await,
      Err(ApplyError::InsufficientStock)
    ));
    assert_eq!(order_status_of(&mut tx, order_id).await, "cancelled");
    tx.rollback().await.unwrap();
  }

  #[tokio::test]
  async fn reopen_rejected_once_paid() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 2).await;
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    apply(&mut tx, order_id, OrderEvent::Pay, opts)
      .await
      .unwrap();
    apply(&mut tx, order_id, OrderEvent::Cancel, opts)
      .await
      .unwrap();
    assert!(matches!(
      apply(&mut tx, order_id, OrderEvent::Reopen, opts).await,
      Err(ApplyError::Illegal { .. })
    ));
    assert!(matches!(
      apply(&mut tx, order_id, OrderEvent::ReopenPay, opts).await,
      Err(ApplyError::Illegal { .. })
    ));
    tx.rollback().await.unwrap();
  }

  #[tokio::test]
  async fn reopen_reclaims_coupon() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 2).await;
    let coupon_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO coupons (id, code, discount_type, discount_value)
       VALUES ($1, $2, 'fixed', 100)",
    )
    .bind(coupon_id)
    .bind(format!("reabrir-{}", coupon_id.simple()))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
      "INSERT INTO coupon_redemptions (id, coupon_id, order_id, guest_email)
       VALUES ($1, $2, $3, 'a@b.co')",
    )
    .bind(Uuid::now_v7())
    .bind(coupon_id)
    .bind(order_id)
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let opts = ApplyOptions::default();
    // Cancelling restores the redemption...
    apply(&mut tx, order_id, OrderEvent::Cancel, opts)
      .await
      .unwrap();
    let refunded: Option<String> = sqlx::query_scalar(
      "SELECT refunded_at::text FROM coupon_redemptions WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(refunded.is_some());
    // ...and reopening consumes the coupon again.
    apply(&mut tx, order_id, OrderEvent::Reopen, opts)
      .await
      .unwrap();
    let refunded: Option<String> = sqlx::query_scalar(
      "SELECT refunded_at::text FROM coupon_redemptions WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(refunded.is_none());
    tx.commit().await.unwrap();
  }

  #[tokio::test]
  async fn apply_with_audit_records_trail() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, _, _) = seed_order_with_stock(&pool, 2).await;
    let actor_id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, $2, $3)")
      .bind(actor_id)
      .bind("Admin")
      .bind(format!("admin-{}@example.com", actor_id.simple()))
      .execute(&pool)
      .await
      .unwrap();
    let mut tx = pool.begin().await.unwrap();
    apply_with_audit(
      &mut tx,
      order_id,
      OrderEvent::Pay,
      ApplyOptions::default(),
      Some(actor_id),
      Some("Admin"),
    )
    .await
    .unwrap();
    apply_with_audit(
      &mut tx,
      order_id,
      OrderEvent::Refund,
      ApplyOptions::with_restock(true),
      Some(actor_id),
      Some("Admin"),
    )
    .await
    .unwrap();
    let rows: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
      "SELECT aggregate_type, entity_type, metadata FROM audit_events
       WHERE entity_id = $1 ORDER BY occurred_at",
    )
    .bind(order_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "order");
    assert_eq!(rows[0].1, "order");
    assert_eq!(rows[0].2["action"], serde_json::json!("pay"));
    assert_eq!(rows[0].2["from"], serde_json::json!("pending_payment"));
    assert_eq!(rows[0].2["to"], serde_json::json!("paid"));
    assert_eq!(rows[0].2["restock"], serde_json::json!(false));
    assert_eq!(rows[1].2["action"], serde_json::json!("refund"));
    assert_eq!(rows[1].2["from"], serde_json::json!("paid"));
    assert_eq!(rows[1].2["to"], serde_json::json!("refunded"));
    assert_eq!(rows[1].2["restock"], serde_json::json!(true));
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
