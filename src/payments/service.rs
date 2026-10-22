//! Order ↔ payment orchestration: create the charge for a persisted order,
//! sync a PSP payment into `order.apply`, admin sync/refund, and the
//! payment-aware expiry sweep.
//!
//! Amounts, payer and expiry are always read from the `orders` row, never
//! from a request. PSP state reaches the order only through
//! [`apply_with_audit`], which is idempotent here because the event is
//! derived from the order's *current* status under `SELECT … FOR UPDATE`
//! (an `approved` payment applied twice finds the order already `paid`).

use sqlx::PgPool;
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;

use super::{
  ChargeStatus, NewCharge, PaymentError, PaymentInfo, PaymentProvider,
};
use crate::app::store::order::{
  ApplyError, ApplyOptions, OrderEvent, OrderStatus, apply_with_audit,
};

/// Who triggered a payment-driven transition (audit trail).
#[derive(Debug, Clone, Copy)]
pub struct Actor<'a> {
  pub id: Option<Uuid>,
  pub name: &'a str,
}

/// Webhook / sweeper transitions are attributed to the PSP itself.
pub const MERCADO_PAGO_ACTOR: Actor<'static> = Actor {
  id: None,
  name: "Mercado Pago",
};

/// Past the hold by this much, the sweep cancels an MP order even when the
/// MP API cannot be reached (MP's own PIX expiry has passed too; a late
/// approval still lands through the webhook as `ReopenPay`).
const SWEEP_GRACE_SECS: i64 = 3600;

#[derive(Debug)]
pub enum ServiceError {
  Payment(PaymentError),
  Apply(ApplyError),
  Db(sqlx::Error),
}

impl From<PaymentError> for ServiceError {
  fn from(error: PaymentError) -> Self {
    Self::Payment(error)
  }
}

impl From<ApplyError> for ServiceError {
  fn from(error: ApplyError) -> Self {
    Self::Apply(error)
  }
}

impl From<sqlx::Error> for ServiceError {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

impl std::fmt::Display for ServiceError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Payment(error) => write!(f, "{error}"),
      Self::Apply(error) => write!(f, "{error}"),
      Self::Db(error) => write!(f, "{error}"),
    }
  }
}

/// Result of applying a PSP payment state to its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
  Applied {
    order_id: Uuid,
    from: OrderStatus,
    to: OrderStatus,
  },
  /// Nothing to do (already applied, or a non-terminal PSP status).
  Unchanged { order_id: Uuid },
  /// No order references this payment.
  UnknownPayment,
  /// The payment's order reference or amount disagrees with the order.
  Mismatch { order_id: Uuid },
  /// Approved after the order was cancelled and its stock is gone: needs
  /// an admin refund.
  NeedsAttention { order_id: Uuid },
}

/// PIX instructions for the order page.
#[derive(Debug, Clone)]
pub struct ChargeView {
  pub payment_id: i64,
  pub qr_code: Option<String>,
  pub qr_code_base64: Option<String>,
  pub ticket_url: Option<String>,
  /// `dd/mm/aaaa às HH:MM` (Brasília time).
  pub expires_label: Option<String>,
}

/// How the order page should present payment.
#[derive(Debug, Clone)]
pub enum OrderPayment {
  StaticPix,
  /// `None` = charge not available yet (PSP down); the page retries on
  /// the next load.
  MercadoPago(Option<ChargeView>),
}

/// Map PSP state to an order event given the order's current status.
/// `None` means "no transition" — which is what makes redelivery safe.
pub fn event_for(
  from: OrderStatus,
  ever_paid: bool,
  status: ChargeStatus,
) -> Option<OrderEvent> {
  match (status, from) {
    (ChargeStatus::Approved, OrderStatus::PendingPayment) => {
      Some(OrderEvent::Pay)
    }
    // Paid after the hold expired and the sweep cancelled the order.
    (ChargeStatus::Approved, OrderStatus::Cancelled) if !ever_paid => {
      Some(OrderEvent::ReopenPay)
    }
    (ChargeStatus::Failed, OrderStatus::PendingPayment) => {
      Some(OrderEvent::Cancel)
    }
    (
      ChargeStatus::Refunded,
      OrderStatus::Paid
      | OrderStatus::Processing
      | OrderStatus::Shipped
      | OrderStatus::Delivered,
    ) => Some(OrderEvent::Refund),
    _ => None,
  }
}

fn brasilia_label(at: OffsetDateTime) -> String {
  let offset = UtcOffset::from_hms(-3, 0, 0).expect("valid offset");
  let local = at.to_offset(offset);
  format!(
    "{:02}/{:02}/{:04} às {:02}:{:02}",
    local.day(),
    u8::from(local.month()),
    local.year(),
    local.hour(),
    local.minute()
  )
}

#[derive(sqlx::FromRow)]
struct ChargeRow {
  payment_provider: String,
  status: String,
  total_cents: i64,
  guest_email: Option<String>,
  reservation_expires_at: Option<OffsetDateTime>,
  mp_payment_id: Option<i64>,
  mp_qr_code: Option<String>,
  mp_qr_code_base64: Option<String>,
  mp_ticket_url: Option<String>,
  mp_expires_at: Option<OffsetDateTime>,
}

impl ChargeRow {
  fn view(&self) -> Option<ChargeView> {
    self.mp_payment_id.map(|payment_id| ChargeView {
      payment_id,
      qr_code: self.mp_qr_code.clone(),
      qr_code_base64: self.mp_qr_code_base64.clone(),
      ticket_url: self.mp_ticket_url.clone(),
      expires_label: self.mp_expires_at.map(brasilia_label),
    })
  }
}

async fn load_charge_row(
  pool: &PgPool,
  order_id: Uuid,
) -> Result<Option<ChargeRow>, sqlx::Error> {
  sqlx::query_as::<_, ChargeRow>(
    "SELECT payment_provider, status::text AS status, total_cents, guest_email,
            reservation_expires_at, mp_payment_id, mp_qr_code, mp_qr_code_base64,
            mp_ticket_url, mp_expires_at
     FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_optional(pool)
  .await
}

/// Create the PSP charge for a pending Mercado Pago order unless it already
/// has one. Safe to call repeatedly: the idempotency key is the order id.
pub async fn ensure_charge(
  pool: &PgPool,
  provider: &PaymentProvider,
  order_id: Uuid,
) -> Result<Option<ChargeView>, ServiceError> {
  let Some(row) = load_charge_row(pool, order_id).await? else {
    return Ok(None);
  };
  if row.payment_provider != super::ProviderKind::MercadoPago.as_str() {
    return Ok(None);
  }
  if let Some(view) = row.view() {
    return Ok(Some(view));
  }
  if row.status != OrderStatus::PendingPayment.as_str() {
    return Ok(None);
  }
  let expires_at = row
    .reservation_expires_at
    .unwrap_or_else(|| OffsetDateTime::now_utc() + time::Duration::hours(24));
  let short_id: String = order_id.to_string().chars().take(8).collect();
  let charge = provider
    .create_charge(&NewCharge {
      order_id,
      amount_cents: row.total_cents,
      payer_email: row.guest_email.clone().unwrap_or_default(),
      expires_at,
      description: format!("Pedido Linda Flor #{short_id}"),
    })
    .await?;
  sqlx::query(
    "UPDATE orders
     SET mp_payment_id = $2, mp_status = $3, mp_status_detail = $4,
         mp_qr_code = $5, mp_qr_code_base64 = $6, mp_ticket_url = $7,
         mp_expires_at = $8, mp_synced_at = now(), updated_at = now()
     WHERE id = $1 AND (mp_payment_id IS NULL OR mp_payment_id = $2)",
  )
  .bind(order_id)
  .bind(charge.payment_id)
  .bind(&charge.raw_status)
  .bind(charge.status_detail.as_deref())
  .bind(charge.qr_code.as_deref())
  .bind(charge.qr_code_base64.as_deref())
  .bind(charge.ticket_url.as_deref())
  .bind(expires_at)
  .execute(pool)
  .await?;
  tracing::info!(
    %order_id,
    mp_payment_id = charge.payment_id,
    status = %charge.raw_status,
    "mercado pago charge created"
  );
  Ok(Some(ChargeView {
    payment_id: charge.payment_id,
    qr_code: charge.qr_code,
    qr_code_base64: charge.qr_code_base64,
    ticket_url: charge.ticket_url,
    expires_label: Some(brasilia_label(expires_at)),
  }))
}

/// Payment presentation for `/pedido/{id}`. Lazily (re)tries the charge for
/// a pending MP order that has none yet (e.g. MP was down at checkout).
pub async fn order_payment(
  pool: &PgPool,
  provider: &PaymentProvider,
  order_id: Uuid,
) -> Result<OrderPayment, sqlx::Error> {
  let Some(row) = load_charge_row(pool, order_id).await? else {
    return Ok(OrderPayment::StaticPix);
  };
  if row.payment_provider != super::ProviderKind::MercadoPago.as_str() {
    return Ok(OrderPayment::StaticPix);
  }
  if let Some(view) = row.view() {
    return Ok(OrderPayment::MercadoPago(Some(view)));
  }
  if row.status != OrderStatus::PendingPayment.as_str() {
    return Ok(OrderPayment::MercadoPago(None));
  }
  match ensure_charge(pool, provider, order_id).await {
    Ok(view) => Ok(OrderPayment::MercadoPago(view)),
    Err(ServiceError::Db(error)) => Err(error),
    Err(error) => {
      tracing::warn!(%order_id, %error, "mercado pago charge not available");
      Ok(OrderPayment::MercadoPago(None))
    }
  }
}

/// Order for a PSP payment: by `mp_payment_id`, else by the payment's
/// `external_reference` (our order id, read from the MP API — not from the
/// webhook body) when the id was never stored (crash between the MP call
/// and the UPDATE).
async fn find_order(
  pool: &PgPool,
  info: &PaymentInfo,
) -> Result<Option<Uuid>, sqlx::Error> {
  let by_id: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM orders WHERE mp_payment_id = $1")
      .bind(info.payment_id)
      .fetch_optional(pool)
      .await?;
  if by_id.is_some() {
    return Ok(by_id);
  }
  let Some(order_id) = info
    .external_reference
    .as_deref()
    .and_then(|reference| Uuid::parse_str(reference.trim()).ok())
  else {
    return Ok(None);
  };
  sqlx::query_scalar(
    "UPDATE orders SET mp_payment_id = $2
     WHERE id = $1 AND payment_provider = 'mercadopago' AND mp_payment_id IS NULL
     RETURNING id",
  )
  .bind(order_id)
  .bind(info.payment_id)
  .fetch_optional(pool)
  .await
}

/// Fetch a payment from the PSP and apply it to its order.
pub async fn sync_payment(
  pool: &PgPool,
  provider: &PaymentProvider,
  payment_id: i64,
  actor: Actor<'_>,
) -> Result<SyncOutcome, ServiceError> {
  let info = provider.fetch_status(payment_id).await?;
  apply_payment_info(pool, &info, actor).await
}

/// Apply PSP payment state (as fetched from the API) to its order, in one
/// transaction under the order row lock.
pub async fn apply_payment_info(
  pool: &PgPool,
  info: &PaymentInfo,
  actor: Actor<'_>,
) -> Result<SyncOutcome, ServiceError> {
  let Some(order_id) = find_order(pool, info).await? else {
    tracing::warn!(
      mp_payment_id = info.payment_id,
      "mercado pago payment has no matching order"
    );
    return Ok(SyncOutcome::UnknownPayment);
  };

  let mut tx = pool.begin().await?;
  let (raw_status, ever_paid, total_cents): (String, bool, i64) =
    sqlx::query_as(
      "SELECT status::text, paid_at IS NOT NULL, total_cents
       FROM orders WHERE id = $1 FOR UPDATE",
    )
    .bind(order_id)
    .fetch_one(&mut *tx)
    .await?;
  let from = OrderStatus::parse(&raw_status).ok_or(ApplyError::NotFound)?;

  if info
    .external_reference
    .as_deref()
    .is_some_and(|reference| reference.trim() != order_id.to_string())
  {
    tracing::error!(
      %order_id,
      mp_payment_id = info.payment_id,
      "mercado pago external_reference does not match the order"
    );
    let _ = tx.rollback().await;
    return Ok(SyncOutcome::Mismatch { order_id });
  }

  sqlx::query(
    "UPDATE orders SET mp_status = $2, mp_status_detail = $3, mp_synced_at = now()
     WHERE id = $1",
  )
  .bind(order_id)
  .bind(&info.raw_status)
  .bind(info.status_detail.as_deref())
  .execute(&mut *tx)
  .await?;

  let Some(event) = event_for(from, ever_paid, info.status) else {
    tx.commit().await?;
    return Ok(SyncOutcome::Unchanged { order_id });
  };

  // Never mark paid on an amount we did not ask for.
  if matches!(event, OrderEvent::Pay | OrderEvent::ReopenPay)
    && info.amount_cents != Some(total_cents)
  {
    tracing::error!(
      %order_id,
      mp_payment_id = info.payment_id,
      "mercado pago approved amount does not match the order total"
    );
    tx.commit().await?;
    return Ok(SyncOutcome::Mismatch { order_id });
  }

  match apply_with_audit(
    &mut tx,
    order_id,
    event,
    ApplyOptions::default(),
    actor.id,
    Some(actor.name),
  )
  .await
  {
    Ok(to) => {
      tx.commit().await?;
      tracing::info!(
        %order_id,
        mp_payment_id = info.payment_id,
        new_status = to.as_str(),
        "mercado pago payment applied"
      );
      Ok(SyncOutcome::Applied { order_id, from, to })
    }
    Err(ApplyError::InsufficientStock) => {
      let _ = tx.rollback().await;
      sqlx::query(
        "UPDATE orders SET mp_status = $2, mp_status_detail = $3, mp_synced_at = now()
         WHERE id = $1",
      )
      .bind(order_id)
      .bind(&info.raw_status)
      .bind(info.status_detail.as_deref())
      .execute(pool)
      .await?;
      tracing::error!(
        %order_id,
        mp_payment_id = info.payment_id,
        "late mercado pago approval but stock is gone: refund required"
      );
      Ok(SyncOutcome::NeedsAttention { order_id })
    }
    Err(error) => {
      let _ = tx.rollback().await;
      Err(error.into())
    }
  }
}

/// Admin view of an order's payment.
#[derive(Debug, Clone)]
pub struct AdminPaymentInfo {
  pub provider: super::ProviderKind,
  pub mp_payment_id: Option<i64>,
  pub mp_status: Option<String>,
  pub mp_status_detail: Option<String>,
  pub synced_at: Option<String>,
}

impl AdminPaymentInfo {
  /// Refund is offered for paid-through-delivered orders, and for an
  /// approved payment on a cancelled order (late approval without stock).
  pub fn refundable(&self, order_status: &str) -> bool {
    self.mp_payment_id.is_some()
      && (matches!(
        order_status,
        "paid" | "processing" | "shipped" | "delivered"
      ) || (order_status == "cancelled"
        && self.mp_status.as_deref() == Some("approved")))
  }
}

#[derive(sqlx::FromRow)]
struct AdminPaymentRow {
  payment_provider: String,
  mp_payment_id: Option<i64>,
  mp_status: Option<String>,
  mp_status_detail: Option<String>,
  mp_synced_at: Option<OffsetDateTime>,
}

pub async fn admin_payment_info(
  pool: &PgPool,
  order_id: Uuid,
) -> Result<Option<AdminPaymentInfo>, sqlx::Error> {
  let row = sqlx::query_as::<_, AdminPaymentRow>(
    "SELECT payment_provider, mp_payment_id, mp_status, mp_status_detail, mp_synced_at
     FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_optional(pool)
  .await?;
  Ok(row.map(|row| AdminPaymentInfo {
    provider: if row.payment_provider == "mercadopago" {
      super::ProviderKind::MercadoPago
    } else {
      super::ProviderKind::StaticPix
    },
    mp_payment_id: row.mp_payment_id,
    mp_status: row.mp_status,
    mp_status_detail: row.mp_status_detail,
    synced_at: row.mp_synced_at.map(brasilia_label),
  }))
}

/// Admin buttons on the order page, posted through the existing
/// `POST /admin/pedidos/{id}` form as `status=mp_sync|mp_refund`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminAction {
  Sync,
  Refund,
}

impl AdminAction {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw {
      "mp_sync" => Some(Self::Sync),
      "mp_refund" => Some(Self::Refund),
      _ => None,
    }
  }
}

/// Run an admin payment action. `Ok` / `Err` carry the pt-BR toast text.
/// The caller has already checked `require_admin`.
pub async fn run_admin_action(
  pool: &PgPool,
  provider: &PaymentProvider,
  order_id: Uuid,
  action: AdminAction,
  actor: Actor<'_>,
) -> Result<String, String> {
  let unavailable = || "Não foi possível falar com o Mercado Pago.".to_string();
  let info = admin_payment_info(pool, order_id)
    .await
    .map_err(|_| unavailable())?
    .ok_or_else(|| "Pedido não encontrado.".to_string())?;
  let Some(payment_id) = info.mp_payment_id else {
    return Err("Pedido sem pagamento no Mercado Pago.".to_string());
  };
  if provider.mercadopago().is_none() {
    return Err("Mercado Pago não está configurado.".to_string());
  }
  match action {
    AdminAction::Sync => {
      match sync_payment(pool, provider, payment_id, actor).await {
        Ok(SyncOutcome::Applied { .. }) => {
          Ok("Pedido sincronizado com o Mercado Pago.".to_string())
        }
        Ok(SyncOutcome::Unchanged { .. }) => {
          Ok("Pedido já está em dia com o Mercado Pago.".to_string())
        }
        Ok(SyncOutcome::NeedsAttention { .. }) => Err(
          "Pagamento aprovado, mas sem estoque para reabrir o pedido. Estorne o pagamento."
            .to_string(),
        ),
        Ok(SyncOutcome::Mismatch { .. }) => Err(
          "O pagamento não confere com o pedido (referência ou valor). Verifique no Mercado Pago."
            .to_string(),
        ),
        Ok(SyncOutcome::UnknownPayment) => {
          Err("Pagamento não encontrado.".to_string())
        }
        Err(ServiceError::Payment(error)) => Err(error.to_string()),
        Err(error) => {
          tracing::error!(%order_id, %error, "mercado pago sync failed");
          Err(unavailable())
        }
      }
    }
    AdminAction::Refund => {
      let status: Option<String> =
        sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
          .bind(order_id)
          .fetch_optional(pool)
          .await
          .map_err(|_| unavailable())?;
      let status = status.unwrap_or_default();
      if !info.refundable(&status) {
        return Err("Este pedido não pode ser estornado.".to_string());
      }
      provider
        .refund(payment_id)
        .await
        .map_err(|error| error.to_string())?;
      record_refund(pool, order_id, payment_id, actor)
        .await
        .map_err(|error| {
          tracing::error!(%order_id, %error, "refund recorded at MP but not locally");
          "Estorno feito no Mercado Pago, mas o pedido não foi atualizado. Use Sincronizar."
            .to_string()
        })?;
      Ok("Pagamento estornado no Mercado Pago.".to_string())
    }
  }
}

/// After a successful PSP refund: apply `Refund` (stock/coupon side effects
/// via `order.apply`) and leave an audit row for the PSP call itself.
async fn record_refund(
  pool: &PgPool,
  order_id: Uuid,
  payment_id: i64,
  actor: Actor<'_>,
) -> Result<(), ServiceError> {
  let mut tx = pool.begin().await?;
  let raw: String = sqlx::query_scalar(
    "SELECT status::text FROM orders WHERE id = $1 FOR UPDATE",
  )
  .bind(order_id)
  .fetch_one(&mut *tx)
  .await?;
  let from = OrderStatus::parse(&raw).ok_or(ApplyError::NotFound)?;
  if event_for(from, true, ChargeStatus::Refunded).is_some() {
    apply_with_audit(
      &mut tx,
      order_id,
      OrderEvent::Refund,
      ApplyOptions::default(),
      actor.id,
      Some(actor.name),
    )
    .await?;
  }
  sqlx::query(
    "UPDATE orders SET mp_status = 'refunded', mp_synced_at = now() WHERE id = $1",
  )
  .bind(order_id)
  .execute(&mut *tx)
  .await?;
  let aggregate_id = order_id.to_string();
  crate::audit::record_audit_event(
    &mut *tx,
    crate::audit::AuditEvent {
      aggregate_type: "order",
      aggregate_id: aggregate_id.as_str(),
      entity_type: "order",
      entity_id: order_id,
      actor_id: actor.id,
      actor_name: Some(actor.name),
      metadata: serde_json::json!({
        "action": "mp_refund",
        "mp_payment_id": payment_id,
      }),
    },
  )
  .await?;
  tx.commit().await?;
  Ok(())
}

/// Payment-aware reservation sweep (replaces the plain sweep in `main.rs`).
///
/// With Mercado Pago, every expired pending MP order is re-fetched first:
/// an approval that never reached us (lost webhook) marks it paid instead
/// of cancelling. If MP cannot be reached the order is kept until
/// [`SWEEP_GRACE_SECS`] past its hold. Everything else is cancelled one
/// order per transaction via [`crate::app::store::order::expire_one`].
pub async fn expire_pending_reservations(
  pool: &PgPool,
  provider: &PaymentProvider,
) -> Result<(), ApplyError> {
  if provider.mercadopago().is_none() {
    return crate::app::store::order::expire_pending_reservations(pool).await;
  }
  let expired: Vec<(Uuid, Option<i64>, bool)> = sqlx::query_as(
    "SELECT id,
            CASE WHEN payment_provider = 'mercadopago' THEN mp_payment_id END,
            reservation_expires_at < now() - make_interval(secs => $1)
     FROM orders
     WHERE status = 'pending_payment'
       AND reservation_expires_at IS NOT NULL
       AND reservation_expires_at < now()
     ORDER BY id",
  )
  .bind(SWEEP_GRACE_SECS as f64)
  .fetch_all(pool)
  .await?;

  let mut first_error = None;
  for (order_id, mp_payment_id, past_grace) in expired {
    if let Some(payment_id) = mp_payment_id {
      match sync_payment(pool, provider, payment_id, MERCADO_PAGO_ACTOR).await {
        Ok(_) => {}
        Err(error) if !past_grace => {
          tracing::warn!(%order_id, %error, "expiry deferred: mercado pago unreachable");
          continue;
        }
        Err(error) => {
          tracing::warn!(%order_id, %error, "expiring without mercado pago confirmation");
        }
      }
    }
    if let Err(error) =
      crate::app::store::order::expire_one(pool, order_id).await
    {
      tracing::error!(%order_id, %error, "reservation expiry failed");
      first_error.get_or_insert(error);
    }
  }
  first_error.map_or(Ok(()), Err)
}
