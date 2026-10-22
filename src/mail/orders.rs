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
  payment_provider: String,
  carrier: String,
  tracking_code: Option<String>,
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
                    'DD/MM/YYYY HH24:MI') AS pay_until,
            o.payment_provider, o.carrier, o.tracking_code
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
    payment_provider,
    carrier,
    tracking_code,
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
  let mercado_pago =
    payment_provider == crate::payments::ProviderKind::MercadoPago.as_str();
  let tracking_code = tracking_code
    .map(|code| code.trim().to_string())
    .filter(|code| !code.is_empty());
  let tracking_url = tracking_code
    .as_deref()
    .and_then(|code| crate::app::store::order::tracking_url(&carrier, code));
  Ok(Some(Recipient {
    to,
    order: OrderMail {
      code: templates::order_code(order_id),
      name: name.unwrap_or_default(),
      total_cents,
      url,
      // Mercado Pago issues its own (dynamic) QR on the order page; a
      // static BR Code must never reach those customers.
      pix_copy_paste: pix.filter(|p| !p.is_empty() && !mercado_pago),
      pay_until,
      mercado_pago,
      carrier: carrier.trim().to_string(),
      tracking_code,
      tracking_url,
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

#[cfg(test)]
mod tests {
  use super::*;

  async fn seed(pool: &sqlx::PgPool, provider: &str) -> Uuid {
    let order_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO orders (id, guest_email, status, access_token, total_cents,
                           payment_provider, payment_meta)
       VALUES ($1, 'mp@example.com', 'pending_payment', $2, 5000, $3,
               jsonb_build_object('pix_copy_paste', '000201STATICPIXCODE'))",
    )
    .bind(order_id)
    .bind(Uuid::new_v4())
    .bind(provider)
    .execute(pool)
    .await
    .unwrap();
    order_id
  }

  async fn created_body(pool: &sqlx::PgPool, order_id: Uuid) -> String {
    let mut tx = pool.begin().await.unwrap();
    enqueue_order_created(&mut tx, order_id).await.unwrap();
    let body: String = sqlx::query_scalar(
      "SELECT text_body FROM email_outbox WHERE dedupe_key = $1",
    )
    .bind(format!("order:{order_id}:created"))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    body
  }

  #[tokio::test]
  async fn order_created_mail_hides_static_pix_for_mercado_pago() {
    let pool = crate::test_support::fresh_pool().await;

    let mp = seed(&pool, "mercadopago").await;
    let body = created_body(&pool, mp).await;
    assert!(!body.contains("STATICPIXCODE"), "{body}");
    assert!(body.contains("Mercado Pago"), "{body}");
    assert!(body.contains(&format!("/pedido/{mp}?t=")), "{body}");

    let pix = seed(&pool, "static_pix").await;
    let body = created_body(&pool, pix).await;
    assert!(body.contains("000201STATICPIXCODE"), "{body}");
  }
}
