//! Return/exchange mail hooks (see `crate::app::store::returns`). They run
//! inside the caller's transaction so the mail commits with the step.

use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::outbox::{enqueue as enqueue_mail, kind};
use super::templates::{self, ReturnMail};
use crate::app::store::returns::ReturnKind;

/// Which mail to send for a request.
#[derive(Debug, Clone, Copy)]
pub enum ReturnMailKind<'a> {
  Requested,
  /// With the reverse-logistics instructions.
  Approved(&'a str),
  /// With the reason given to the customer.
  Rejected(&'a str),
  /// `Some(cents)` refunded, `None` exchanged.
  Resolved(Option<i64>),
}

#[derive(sqlx::FromRow)]
struct Row {
  order_id: Uuid,
  access_token: Uuid,
  kind: String,
  email: Option<String>,
  name: Option<String>,
}

pub async fn enqueue(
  tx: &mut Transaction<'_, Postgres>,
  return_id: Uuid,
  mail: ReturnMailKind<'_>,
) -> Result<(), sqlx::Error> {
  let row: Option<Row> = sqlx::query_as(
    "SELECT o.id AS order_id, o.access_token, r.kind,
            COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
            COALESCE(NULLIF(btrim(o.shipping_address->>'name'), ''), u.name)
              AS name
     FROM return_requests r
     JOIN orders o ON o.id = r.order_id
     LEFT JOIN users u ON u.id = o.user_id
     WHERE r.id = $1",
  )
  .bind(return_id)
  .fetch_optional(&mut **tx)
  .await?;
  let Some(row) = row else {
    return Ok(());
  };
  let Some(to) = row.email.filter(|e| e.contains('@')) else {
    tracing::warn!(%return_id, "return request has no recipient email");
    return Ok(());
  };
  let items: Vec<(i32, String, String)> = sqlx::query_as(
    "SELECT ri.quantity, oi.product_name, oi.variant_label
     FROM return_request_items ri
     JOIN order_items oi ON oi.id = ri.order_item_id
     WHERE ri.return_request_id = $1
     ORDER BY oi.created_at, oi.id",
  )
  .bind(return_id)
  .fetch_all(&mut **tx)
  .await?;
  let order_id = row.order_id;
  let data = ReturnMail {
    order_code: templates::order_code(order_id),
    name: row.name.unwrap_or_default(),
    kind_label: ReturnKind::parse(&row.kind)
      .map_or("Troca/devolução", ReturnKind::label)
      .to_string(),
    items: items
      .into_iter()
      .map(|(quantity, product, variant)| {
        format!("{quantity}x {product} ({variant})")
      })
      .collect(),
    url: super::absolute_url(&format!(
      "/pedido/{order_id}?t={}",
      super::encode_query_value(&row.access_token.to_string())
    )),
  };
  let (mail_kind, email, step) = match mail {
    ReturnMailKind::Requested => (
      kind::RETURN_REQUESTED,
      templates::return_requested(&data),
      "requested",
    ),
    ReturnMailKind::Approved(instructions) => (
      kind::RETURN_APPROVED,
      templates::return_approved(&data, instructions),
      "approved",
    ),
    ReturnMailKind::Rejected(reason) => (
      kind::RETURN_REJECTED,
      templates::return_rejected(&data, reason),
      "rejected",
    ),
    ReturnMailKind::Resolved(refund) => (
      kind::RETURN_RESOLVED,
      templates::return_resolved(&data, refund),
      "resolved",
    ),
  };
  // Each step happens at most once per request.
  let dedupe = format!("return:{return_id}:{step}");
  enqueue_mail(&mut **tx, mail_kind, &to, &email, Some(&dedupe)).await
}
