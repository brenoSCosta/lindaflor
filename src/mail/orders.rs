//! Commerce mail hooks. Both run inside the caller's order transaction so
//! the mail commits (or rolls back) with the state change.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::outbox::{enqueue, kind};
use super::templates::{self, OrderMail};
use crate::app::store::order::OrderStatus;

#[derive(sqlx::FromRow)]
struct OrderRow {
  access_token: Uuid,
  total_cents: i64,
  email: Option<String>,
  name: Option<String>,
  pix: Option<String>,
  pay_until: Option<String>,
}

struct Recipient {
  to: String,
  order: OrderMail,
}

async fn load(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<Option<Recipient>, sqlx::Error> {
  let row: Option<OrderRow> = sqlx::query_as(
    "SELECT o.access_token,
            o.total_cents::bigint AS total_cents,
            COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
            COALESCE(NULLIF(btrim(o.shipping_address->>'name'), ''), u.name)
              AS name,
            o.payment_meta->>'pix_copy_paste' AS pix,
            to_char(o.reservation_expires_at AT TIME ZONE 'America/Sao_Paulo',
                    'DD/MM/YYYY HH24:MI') AS pay_until
     FROM orders o
     LEFT JOIN users u ON u.id = o.user_id
     WHERE o.id = $1",
  )
  .bind(order_id)
  .fetch_optional(&mut **tx)
  .await?;
  let Some(OrderRow {
    access_token,
    total_cents,
    email,
    name,
    pix,
    pay_until,
  }) = row
  else {
    return Ok(None);
  };
  let Some(to) = email.filter(|e| e.contains('@')) else {
    tracing::warn!(order_id = %order_id, "order has no recipient email");
    return Ok(None);
  };
  let url = super::absolute_url(&format!(
    "/pedido/{order_id}?t={}",
    super::encode_query_value(&access_token.to_string())
  ));
  Ok(Some(Recipient {
    to,
    order: OrderMail {
      code: templates::order_code(order_id),
      name: name.unwrap_or_default(),
      total_cents,
      url,
      pix_copy_paste: pix.filter(|p| !p.is_empty()),
      pay_until,
    },
  }))
}

/// "Pedido recebido" with PIX instructions. Call after `payment_meta` is
/// written, inside the checkout transaction.
pub async fn enqueue_order_created(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
) -> Result<(), sqlx::Error> {
  let Some(recipient) = load(tx, order_id).await? else {
    return Ok(());
  };
  let email = templates::order_created(&recipient.order);
  let dedupe = format!("order:{order_id}:created");
  enqueue(
    &mut **tx,
    kind::ORDER_CREATED,
    &recipient.to,
    &email,
    Some(&dedupe),
  )
  .await
}

/// Customer-facing mail for an order transition, if that edge has one.
/// `expired` marks a `pending_payment → cancelled` caused by the
/// reservation deadline (the sweep) rather than an admin.
pub async fn enqueue_order_transition(
  tx: &mut Transaction<'_, Postgres>,
  order_id: Uuid,
  from: OrderStatus,
  next: OrderStatus,
  expired: bool,
) -> Result<(), sqlx::Error> {
  let kind = match next {
    OrderStatus::Paid => kind::PAYMENT_CONFIRMED,
    OrderStatus::Shipped => kind::ORDER_SHIPPED,
    OrderStatus::Cancelled
      if expired && from == OrderStatus::PendingPayment =>
    {
      kind::ORDER_EXPIRED
    }
    OrderStatus::Cancelled => kind::ORDER_CANCELLED,
    OrderStatus::Refunded => kind::ORDER_REFUNDED,
    OrderStatus::PendingPayment
    | OrderStatus::Processing
    | OrderStatus::Delivered => return Ok(()),
  };
  let Some(recipient) = load(tx, order_id).await? else {
    return Ok(());
  };
  let email = match kind {
    kind::PAYMENT_CONFIRMED => templates::payment_confirmed(&recipient.order),
    kind::ORDER_SHIPPED => templates::order_shipped(&recipient.order),
    kind::ORDER_EXPIRED => templates::order_cancelled(&recipient.order, true),
    kind::ORDER_REFUNDED => templates::order_refunded(&recipient.order),
    _ => templates::order_cancelled(&recipient.order, false),
  };
  enqueue(&mut **tx, kind, &recipient.to, &email, None).await
}
