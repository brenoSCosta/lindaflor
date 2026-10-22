//! Returns and exchanges ("Troca fácil — até 7 dias", policy text in
//! `src/app/trocas_devolucoes.rs`).
//!
//! The customer opens a request from `/pedido/{id}` (owner session or a
//! valid guest link) choosing items, quantities, a type and a reason. The
//! admin (`/admin/trocas`) walks it through
//! `requested → approved → received → resolved (refunded | exchanged)`, or
//! `rejected` at any open step. Every step writes `audit_events`
//! (`aggregate_type = 'return'`) and queues the customer mail in the same
//! transaction.
//!
//! # Window
//!
//! CDC art. 49 counts the 7 days from *receipt*. When the store marked the
//! order delivered we use `delivered_at`. When it never did (common: the
//! admin forgets the "Entregue" click), we assume receipt at
//! `shipped_at + `[`UNCONFIRMED_DELIVERY_GRACE_DAYS`] — Correios PAC's
//! longest interstate quote is ~12 business days, so 15 calendar days
//! never cuts a customer short — and the window runs from there.
//! Requests can be opened as soon as the order ships. Defect claims get
//! [`DEFECT_WINDOW_DAYS`] (CDC art. 26, II: 90 days for durable goods);
//! the 7-day window in the policy text is the arrependimento/troca one.
//!
//! # Refunds
//!
//! The order state machine refunds whole orders only (`Refund` moves the
//! order to `refunded`, and Mercado Pago's refund call is full-payment).
//! So a received `devolução`/`defeito` covering **every unit of the order**
//! is refunded through the order (`OrderEvent::Refund` with the admin's
//! restock choice, or "Estornar no Mercado Pago" for MP orders). A partial
//! return is settled out of band (PIX, or a partial refund in the MP
//! panel): the admin records the amount here and may restock just the
//! returned units; the order keeps its status.

use sqlx::{PgPool, Postgres, Transaction};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use super::inventory::{ReserveLine, restock_lines, restock_sold_items};
use super::order::{ApplyError, ApplyOptions, OrderEvent, apply_with_audit};

/// Days to request a troca/devolução after receipt (store policy, CDC 49).
pub const RETURN_WINDOW_DAYS: i64 = 7;
/// Days to report a defect after receipt (CDC art. 26, II).
pub const DEFECT_WINDOW_DAYS: i64 = 90;
/// Assumed transit time when the order was shipped but never marked
/// delivered (see the module docs).
pub const UNCONFIRMED_DELIVERY_GRACE_DAYS: i64 = 15;

pub const REASON_MAX_CHARS: usize = 1000;
pub const SIZE_MAX_CHARS: usize = 20;
pub const ADMIN_TEXT_MAX_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnKind {
  /// Troca de tamanho.
  Exchange,
  /// Devolução (arrependimento).
  Return,
  /// Defeito.
  Defect,
}

impl ReturnKind {
  pub const ALL: [Self; 3] = [Self::Exchange, Self::Return, Self::Defect];

  pub fn parse(raw: &str) -> Option<Self> {
    match raw {
      "exchange" => Some(Self::Exchange),
      "return" => Some(Self::Return),
      "defect" => Some(Self::Defect),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Exchange => "exchange",
      Self::Return => "return",
      Self::Defect => "defect",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Exchange => "Troca de tamanho",
      Self::Return => "Devolução",
      Self::Defect => "Defeito",
    }
  }

  pub fn window_days(self) -> i64 {
    match self {
      Self::Exchange | Self::Return => RETURN_WINDOW_DAYS,
      Self::Defect => DEFECT_WINDOW_DAYS,
    }
  }

  /// A refund settles devoluções and defects; exchanges end in a new piece.
  pub fn can_refund(self) -> bool {
    matches!(self, Self::Return | Self::Defect)
  }

  pub fn can_exchange(self) -> bool {
    matches!(self, Self::Exchange | Self::Defect)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnStatus {
  Requested,
  Approved,
  Received,
  Resolved,
  Rejected,
}

impl ReturnStatus {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw {
      "requested" => Some(Self::Requested),
      "approved" => Some(Self::Approved),
      "received" => Some(Self::Received),
      "resolved" => Some(Self::Resolved),
      "rejected" => Some(Self::Rejected),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Requested => "requested",
      Self::Approved => "approved",
      Self::Received => "received",
      Self::Resolved => "resolved",
      Self::Rejected => "rejected",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Requested => "Em análise",
      Self::Approved => "Aprovada — aguardando envio",
      Self::Received => "Peça recebida",
      Self::Resolved => "Concluída",
      Self::Rejected => "Recusada",
    }
  }

  pub fn is_open(self) -> bool {
    matches!(self, Self::Requested | Self::Approved | Self::Received)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
  Refunded,
  Exchanged,
}

impl Resolution {
  pub fn parse(raw: &str) -> Option<Self> {
    match raw {
      "refunded" => Some(Self::Refunded),
      "exchanged" => Some(Self::Exchanged),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Refunded => "refunded",
      Self::Exchanged => "exchanged",
    }
  }

  pub fn label(self) -> &'static str {
    match self {
      Self::Refunded => "Reembolsada",
      Self::Exchanged => "Trocada",
    }
  }
}

/// Why a request can't be opened right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Ineligible {
  #[error(
    "A troca ou devolução pode ser solicitada depois que o pedido for enviado."
  )]
  NotShipped,
  #[error("O prazo para solicitar troca ou devolução deste pedido terminou.")]
  Expired,
}

/// End of the request window for `kind`, or `None` while the order has not
/// shipped. See the module docs for the unconfirmed-delivery grace.
pub fn return_deadline(
  kind: ReturnKind,
  delivered_at: Option<OffsetDateTime>,
  shipped_at: Option<OffsetDateTime>,
) -> Option<OffsetDateTime> {
  let received = delivered_at.or_else(|| {
    shipped_at.map(|at| at + Duration::days(UNCONFIRMED_DELIVERY_GRACE_DAYS))
  })?;
  Some(received + Duration::days(kind.window_days()))
}

/// Can a `kind` request be opened for an order in `order_status` at `now`?
/// Returns the deadline when it can.
pub fn check_eligibility(
  order_status: &str,
  kind: ReturnKind,
  delivered_at: Option<OffsetDateTime>,
  shipped_at: Option<OffsetDateTime>,
  now: OffsetDateTime,
) -> Result<OffsetDateTime, Ineligible> {
  if !matches!(order_status, "shipped" | "delivered") {
    return Err(match order_status {
      "pending_payment" | "paid" | "processing" => Ineligible::NotShipped,
      _ => Ineligible::Expired,
    });
  }
  let deadline = return_deadline(kind, delivered_at, shipped_at)
    .ok_or(Ineligible::NotShipped)?;
  if now > deadline {
    return Err(Ineligible::Expired);
  }
  Ok(deadline)
}

#[derive(Debug, thiserror::Error)]
pub enum ReturnError {
  /// Unknown order/request, or the caller may not see it. Same answer
  /// either way.
  #[error("Pedido não encontrado.")]
  NotFound,
  #[error("{0}")]
  Ineligible(#[from] Ineligible),
  #[error("{0}")]
  Invalid(&'static str),
  #[error("Já existe uma solicitação em andamento para este pedido.")]
  AlreadyOpen,
  #[error("Ação inválida para o status atual da solicitação.")]
  IllegalTransition,
  #[error("{0}")]
  Order(ApplyError),
  #[error(transparent)]
  Db(#[from] sqlx::Error),
}

impl From<ApplyError> for ReturnError {
  fn from(error: ApplyError) -> Self {
    match error {
      ApplyError::Db(error) => Self::Db(error),
      other => Self::Order(other),
    }
  }
}

// --- Customer side ---

/// One order line with how many units can still be claimed (quantity minus
/// units in non-rejected requests).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReturnableItem {
  pub order_item_id: Uuid,
  pub product_name: String,
  pub variant_label: String,
  pub quantity: i32,
  pub unit_price_cents: i64,
  pub returnable: i32,
}

pub async fn returnable_items<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  order_id: Uuid,
) -> Result<Vec<ReturnableItem>, sqlx::Error> {
  sqlx::query_as::<_, ReturnableItem>(
    "SELECT oi.id AS order_item_id, oi.product_name, oi.variant_label,
            oi.quantity, oi.unit_price_cents,
            GREATEST(oi.quantity - COALESCE((
                SELECT SUM(ri.quantity)
                FROM return_request_items ri
                JOIN return_requests r ON r.id = ri.return_request_id
                WHERE ri.order_item_id = oi.id AND r.status <> 'rejected'
            ), 0), 0)::int AS returnable
     FROM order_items oi
     WHERE oi.order_id = $1
     ORDER BY oi.created_at, oi.id",
  )
  .bind(order_id)
  .fetch_all(executor)
  .await
}

/// A customer's request as submitted by the form.
#[derive(Debug, Clone)]
pub struct NewReturn {
  pub kind: ReturnKind,
  pub reason: String,
  pub desired_size: Option<String>,
  /// `(order_item_id, quantity)`; zero quantities are ignored.
  pub lines: Vec<(Uuid, i32)>,
}

#[derive(sqlx::FromRow)]
struct OrderAccessRow {
  status: String,
  access_token: Uuid,
  token_expired: bool,
  user_id: Option<Uuid>,
  delivered_at: Option<OffsetDateTime>,
  shipped_at: Option<OffsetDateTime>,
  now: OffsetDateTime,
}

fn clean_text(raw: &str, max: usize) -> Option<String> {
  let trimmed = raw.trim();
  (!trimmed.is_empty() && trimmed.chars().count() <= max)
    .then(|| trimmed.to_string())
}

/// Open a request. Access (owner or guest token), eligibility and the
/// per-item quantity caps are all checked under the order row lock, so two
/// concurrent submits can't claim the same unit twice.
pub async fn create_request(
  pool: &PgPool,
  order_id: Uuid,
  query_t: Option<&str>,
  session_user_id: Option<Uuid>,
  input: NewReturn,
) -> Result<Uuid, ReturnError> {
  let reason = clean_text(&input.reason, REASON_MAX_CHARS).ok_or(
    ReturnError::Invalid("Descreva o motivo (até 1000 caracteres)."),
  )?;
  let desired_size = match input.desired_size.as_deref().map(str::trim) {
    None | Some("") => None,
    Some(size) => Some(
      clean_text(size, SIZE_MAX_CHARS)
        .ok_or(ReturnError::Invalid("Tamanho desejado inválido."))?,
    ),
  };

  let mut tx = pool.begin().await?;
  let row = sqlx::query_as::<_, OrderAccessRow>(
    "SELECT status::text AS status, access_token,
            access_token_expires_at <= now() AS token_expired, user_id,
            delivered_at, shipped_at, now() AS now
     FROM orders WHERE id = $1 FOR UPDATE",
  )
  .bind(order_id)
  .fetch_optional(&mut *tx)
  .await?
  .ok_or(ReturnError::NotFound)?;
  if !crate::app::pedido::id::token_or_owner_allows(
    row.access_token,
    row.token_expired,
    row.user_id,
    query_t,
    session_user_id,
  ) {
    return Err(ReturnError::NotFound);
  }
  check_eligibility(
    &row.status,
    input.kind,
    row.delivered_at,
    row.shipped_at,
    row.now,
  )?;
  let open: bool = sqlx::query_scalar(
    "SELECT EXISTS (SELECT 1 FROM return_requests
                    WHERE order_id = $1
                      AND status IN ('requested', 'approved', 'received'))",
  )
  .bind(order_id)
  .fetch_one(&mut *tx)
  .await?;
  if open {
    return Err(ReturnError::AlreadyOpen);
  }

  let items = returnable_items(&mut *tx, order_id).await?;
  let mut lines: Vec<(Uuid, i32)> = Vec::new();
  for (item_id, quantity) in input.lines {
    if quantity == 0 {
      continue;
    }
    let Some(item) = items.iter().find(|item| item.order_item_id == item_id)
    else {
      return Err(ReturnError::Invalid("Item inválido."));
    };
    if quantity < 0
      || quantity > item.returnable
      || lines.iter().any(|(id, _)| *id == item_id)
    {
      return Err(ReturnError::Invalid("Quantidade inválida."));
    }
    lines.push((item_id, quantity));
  }
  if lines.is_empty() {
    return Err(ReturnError::Invalid(
      "Escolha pelo menos um item e a quantidade.",
    ));
  }

  let return_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO return_requests
         (id, order_id, user_id, kind, reason, desired_size)
     VALUES ($1, $2, $3, $4, $5, $6)",
  )
  .bind(return_id)
  .bind(order_id)
  .bind(session_user_id)
  .bind(input.kind.as_str())
  .bind(&reason)
  .bind(&desired_size)
  .execute(&mut *tx)
  .await?;
  for (item_id, quantity) in &lines {
    sqlx::query(
      "INSERT INTO return_request_items (return_request_id, order_item_id, quantity)
       VALUES ($1, $2, $3)",
    )
    .bind(return_id)
    .bind(item_id)
    .bind(quantity)
    .execute(&mut *tx)
    .await?;
  }
  record(
    &mut tx,
    return_id,
    session_user_id,
    Some("cliente"),
    serde_json::json!({
      "action": "create",
      "order_id": order_id,
      "kind": input.kind.as_str(),
      "to": ReturnStatus::Requested.as_str(),
      "items": lines
        .iter()
        .map(|(id, qty)| serde_json::json!({ "order_item_id": id, "quantity": qty }))
        .collect::<Vec<_>>(),
    }),
  )
  .await?;
  crate::mail::returns::enqueue(
    &mut tx,
    return_id,
    crate::mail::returns::ReturnMailKind::Requested,
  )
  .await?;
  tx.commit().await?;
  Ok(return_id)
}

/// A request as listed on the order page and in the admin.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReturnSummary {
  pub id: Uuid,
  pub order_id: Uuid,
  pub kind: String,
  pub status: String,
  pub resolution: Option<String>,
  pub created_at: OffsetDateTime,
  pub refund_cents: Option<i64>,
  pub instructions: Option<String>,
  pub rejection_reason: Option<String>,
}

impl ReturnSummary {
  pub fn kind_label(&self) -> &'static str {
    ReturnKind::parse(&self.kind).map_or("—", ReturnKind::label)
  }

  /// Status, or the resolution once resolved.
  pub fn status_label(&self) -> &'static str {
    match (
      ReturnStatus::parse(&self.status),
      self.resolution.as_deref().and_then(Resolution::parse),
    ) {
      (Some(ReturnStatus::Resolved), Some(resolution)) => resolution.label(),
      (Some(status), _) => status.label(),
      (None, _) => "—",
    }
  }
}

const SUMMARY_COLUMNS: &str =
  "r.id, r.order_id, r.kind, r.status, r.resolution,
  r.created_at, r.refund_cents, r.instructions, r.rejection_reason";

pub async fn requests_for_order(
  pool: &PgPool,
  order_id: Uuid,
) -> Result<Vec<ReturnSummary>, sqlx::Error> {
  sqlx::query_as::<_, ReturnSummary>(&format!(
    "SELECT {SUMMARY_COLUMNS} FROM return_requests r
     WHERE r.order_id = $1 ORDER BY r.created_at DESC"
  ))
  .bind(order_id)
  .fetch_all(pool)
  .await
}

/// What the order page needs: existing requests plus, when a new one can
/// be opened, the kinds still inside their window with their deadlines.
#[derive(Debug, Clone, Default)]
pub struct OrderReturns {
  /// Show the section at all (shipped/delivered orders, or any request).
  pub visible: bool,
  pub requests: Vec<ReturnSummary>,
  pub open_kinds: Vec<(ReturnKind, OffsetDateTime)>,
  /// Why no request can be opened (shown instead of the link), if any.
  pub blocked: Option<String>,
}

impl OrderReturns {
  pub fn can_open(&self) -> bool {
    self.blocked.is_none() && !self.open_kinds.is_empty()
  }
}

/// Load [`OrderReturns`]. The caller has already checked access.
pub async fn order_returns(
  pool: &PgPool,
  order_id: Uuid,
) -> Result<OrderReturns, sqlx::Error> {
  let row: Option<(
    String,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
    OffsetDateTime,
  )> = sqlx::query_as(
    "SELECT status::text, delivered_at, shipped_at, now() FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_optional(pool)
  .await?;
  let Some((status, delivered_at, shipped_at, now)) = row else {
    return Ok(OrderReturns::default());
  };
  let requests = requests_for_order(pool, order_id).await?;
  let mut open_kinds = Vec::new();
  let mut first_error = None;
  for kind in ReturnKind::ALL {
    match check_eligibility(&status, kind, delivered_at, shipped_at, now) {
      Ok(deadline) => open_kinds.push((kind, deadline)),
      Err(error) => {
        first_error.get_or_insert(error);
      }
    }
  }
  let has_open = requests.iter().any(|request| {
    ReturnStatus::parse(&request.status).is_some_and(ReturnStatus::is_open)
  });
  let blocked = if has_open {
    Some(
      "Você já tem uma solicitação em andamento para este pedido.".to_string(),
    )
  } else if open_kinds.is_empty() {
    first_error.map(|error| error.to_string())
  } else if !returnable_items(pool, order_id)
    .await?
    .iter()
    .any(|item| item.returnable > 0)
  {
    Some("Todos os itens deste pedido já estão em uma solicitação.".to_string())
  } else {
    None
  };
  Ok(OrderReturns {
    visible: !requests.is_empty()
      || matches!(status.as_str(), "shipped" | "delivered"),
    requests,
    open_kinds,
    blocked,
  })
}

// --- Admin side ---

/// One admin step on a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminReturnAction {
  /// `requested → approved`; `instructions` go to the customer (postage /
  /// reverse-logistics code, address, deadline).
  Approve { instructions: String },
  /// Any open status → `rejected`.
  Reject { reason: String },
  /// `approved → received`.
  MarkReceived,
  /// `received → resolved (exchanged)`: the new piece ships out of band.
  ResolveExchanged,
  /// `received → resolved (refunded)` through the order state machine:
  /// static-PIX orders only, and only when the request covers the whole
  /// order (see the module docs). Mercado Pago orders go through
  /// [`finish_mp_refund`] after the PSP call.
  RefundOrder { restock: bool },
  /// `received → resolved (refunded)` recorded only: money returned out of
  /// band (PIX / MP panel). For partial returns, or when the order was
  /// already refunded elsewhere. `restock` puts the returned units back.
  RecordRefund { refund_cents: i64, restock: bool },
  /// Free-text admin note (audit trail only, no status change).
  Note { text: String },
}

impl AdminReturnAction {
  fn name(&self) -> &'static str {
    match self {
      Self::Approve { .. } => "approve",
      Self::Reject { .. } => "reject",
      Self::MarkReceived => "receive",
      Self::ResolveExchanged => "resolve_exchanged",
      Self::RefundOrder { .. } => "refund_order",
      Self::RecordRefund { .. } => "record_refund",
      Self::Note { .. } => "note",
    }
  }
}

/// Target status for `action` from `from`, or `None` when illegal.
/// `Note` keeps the status.
pub fn admin_transition(
  from: ReturnStatus,
  kind: ReturnKind,
  action: &AdminReturnAction,
) -> Option<ReturnStatus> {
  use AdminReturnAction as A;
  use ReturnStatus as S;
  match (from, action) {
    (_, A::Note { .. }) => Some(from),
    (S::Requested, A::Approve { .. }) => Some(S::Approved),
    (S::Requested | S::Approved | S::Received, A::Reject { .. }) => {
      Some(S::Rejected)
    }
    (S::Approved, A::MarkReceived) => Some(S::Received),
    (S::Received, A::ResolveExchanged) if kind.can_exchange() => {
      Some(S::Resolved)
    }
    (S::Received, A::RefundOrder { .. } | A::RecordRefund { .. })
      if kind.can_refund() =>
    {
      Some(S::Resolved)
    }
    _ => None,
  }
}

#[derive(sqlx::FromRow)]
struct LockedReturn {
  order_id: Uuid,
  kind: String,
  status: String,
}

async fn lock_return(
  tx: &mut Transaction<'_, Postgres>,
  return_id: Uuid,
) -> Result<(Uuid, ReturnKind, ReturnStatus), ReturnError> {
  let row = sqlx::query_as::<_, LockedReturn>(
    "SELECT order_id, kind, status FROM return_requests WHERE id = $1 FOR UPDATE",
  )
  .bind(return_id)
  .fetch_optional(&mut **tx)
  .await?
  .ok_or(ReturnError::NotFound)?;
  let kind = ReturnKind::parse(&row.kind).ok_or(ReturnError::NotFound)?;
  let status = ReturnStatus::parse(&row.status).ok_or(ReturnError::NotFound)?;
  Ok((row.order_id, kind, status))
}

/// Returned lines of a request: `(variant_id, quantity, unit_price_cents)`.
async fn returned_lines(
  tx: &mut Transaction<'_, Postgres>,
  return_id: Uuid,
) -> Result<Vec<(Uuid, i32, i64)>, sqlx::Error> {
  sqlx::query_as(
    "SELECT oi.variant_id, ri.quantity, oi.unit_price_cents
     FROM return_request_items ri
     JOIN order_items oi ON oi.id = ri.order_item_id
     WHERE ri.return_request_id = $1
     ORDER BY oi.variant_id",
  )
  .bind(return_id)
  .fetch_all(&mut **tx)
  .await
}

/// Does the request cover every unit of its order?
async fn covers_whole_order<'e>(
  executor: impl sqlx::PgExecutor<'e>,
  return_id: Uuid,
) -> Result<bool, sqlx::Error> {
  sqlx::query_scalar(
    "SELECT NOT EXISTS (
         SELECT 1 FROM order_items oi
         JOIN return_requests r ON r.order_id = oi.order_id
         LEFT JOIN return_request_items ri
           ON ri.order_item_id = oi.id AND ri.return_request_id = r.id
         WHERE r.id = $1 AND COALESCE(ri.quantity, 0) <> oi.quantity
     )",
  )
  .bind(return_id)
  .fetch_one(executor)
  .await
}

/// Refund paths offered for a received request (admin page buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefundOptions {
  /// The request covers the whole order and the order is still refundable:
  /// "Estornar pedido" (static PIX) / "Estornar no Mercado Pago".
  pub full_order: bool,
  pub mercado_pago: bool,
  /// Max amount for [`AdminReturnAction::RecordRefund`] (value of the
  /// returned units, capped at the order total).
  pub max_manual_cents: i64,
  /// The order itself is already `refunded`.
  pub order_refunded: bool,
}

pub async fn refund_options(
  pool: &PgPool,
  return_id: Uuid,
) -> Result<RefundOptions, sqlx::Error> {
  let (order_status, provider, total_cents, items_cents): (
    String,
    String,
    i64,
    i64,
  ) = sqlx::query_as(
    "SELECT o.status::text, o.payment_provider, o.total_cents,
            COALESCE((SELECT SUM(ri.quantity * oi.unit_price_cents)
                      FROM return_request_items ri
                      JOIN order_items oi ON oi.id = ri.order_item_id
                      WHERE ri.return_request_id = r.id), 0)::bigint
     FROM return_requests r JOIN orders o ON o.id = r.order_id
     WHERE r.id = $1",
  )
  .bind(return_id)
  .fetch_one(pool)
  .await?;
  let whole = covers_whole_order(pool, return_id).await?;
  Ok(RefundOptions {
    full_order: whole
      && matches!(order_status.as_str(), "shipped" | "delivered"),
    mercado_pago: provider
      == crate::payments::ProviderKind::MercadoPago.as_str(),
    max_manual_cents: items_cents.min(total_cents).max(0),
    order_refunded: order_status == "refunded",
  })
}

async fn record(
  tx: &mut Transaction<'_, Postgres>,
  return_id: Uuid,
  actor_id: Option<Uuid>,
  actor_name: Option<&str>,
  metadata: serde_json::Value,
) -> Result<(), sqlx::Error> {
  crate::audit::record_audit_event(
    &mut **tx,
    crate::audit::AuditEvent {
      aggregate_type: "return",
      aggregate_id: &return_id.to_string(),
      entity_type: "return_request",
      entity_id: return_id,
      actor_id,
      actor_name,
      metadata,
    },
  )
  .await
}

/// Apply one admin step in a transaction: status change, side effects
/// (order refund / restock), audit row and customer mail commit together.
pub async fn admin_apply(
  pool: &PgPool,
  return_id: Uuid,
  action: AdminReturnAction,
  actor_id: Option<Uuid>,
  actor_name: Option<&str>,
) -> Result<ReturnStatus, ReturnError> {
  let mut tx = pool.begin().await?;
  let next =
    admin_apply_in(&mut tx, return_id, action, actor_id, actor_name).await?;
  tx.commit().await?;
  Ok(next)
}

async fn admin_apply_in(
  tx: &mut Transaction<'_, Postgres>,
  return_id: Uuid,
  action: AdminReturnAction,
  actor_id: Option<Uuid>,
  actor_name: Option<&str>,
) -> Result<ReturnStatus, ReturnError> {
  use crate::mail::returns::ReturnMailKind;

  let (order_id, kind, from) = lock_return(tx, return_id).await?;
  let next = admin_transition(from, kind, &action)
    .ok_or(ReturnError::IllegalTransition)?;
  let mut metadata = serde_json::json!({
    "action": action.name(),
    "order_id": order_id,
    "from": from.as_str(),
    "to": next.as_str(),
  });

  match &action {
    AdminReturnAction::Note { text } => {
      let text = clean_text(text, ADMIN_TEXT_MAX_CHARS)
        .ok_or(ReturnError::Invalid("Escreva a observação."))?;
      metadata["note"] = serde_json::json!(text);
      sqlx::query(
        "UPDATE return_requests SET updated_at = now() WHERE id = $1",
      )
      .bind(return_id)
      .execute(&mut **tx)
      .await?;
      record(tx, return_id, actor_id, actor_name, metadata).await?;
      return Ok(next);
    }
    AdminReturnAction::Approve { instructions } => {
      let instructions = clean_text(instructions, ADMIN_TEXT_MAX_CHARS).ok_or(
        ReturnError::Invalid(
          "Informe as instruções de envio (código de postagem, endereço, prazo).",
        ),
      )?;
      sqlx::query(
        "UPDATE return_requests
         SET status = 'approved', instructions = $2, approved_at = now(), updated_at = now()
         WHERE id = $1",
      )
      .bind(return_id)
      .bind(&instructions)
      .execute(&mut **tx)
      .await?;
      crate::mail::returns::enqueue(
        tx,
        return_id,
        ReturnMailKind::Approved(&instructions),
      )
      .await?;
    }
    AdminReturnAction::Reject { reason } => {
      let reason = clean_text(reason, ADMIN_TEXT_MAX_CHARS).ok_or(
        ReturnError::Invalid("Informe o motivo da recusa para o cliente."),
      )?;
      metadata["reason"] = serde_json::json!(reason);
      sqlx::query(
        "UPDATE return_requests
         SET status = 'rejected', rejection_reason = $2, updated_at = now()
         WHERE id = $1",
      )
      .bind(return_id)
      .bind(&reason)
      .execute(&mut **tx)
      .await?;
      crate::mail::returns::enqueue(
        tx,
        return_id,
        ReturnMailKind::Rejected(&reason),
      )
      .await?;
    }
    AdminReturnAction::MarkReceived => {
      sqlx::query(
        "UPDATE return_requests
         SET status = 'received', received_at = now(), updated_at = now()
         WHERE id = $1",
      )
      .bind(return_id)
      .execute(&mut **tx)
      .await?;
    }
    AdminReturnAction::ResolveExchanged => {
      resolve(tx, return_id, Resolution::Exchanged, None).await?;
      crate::mail::returns::enqueue(
        tx,
        return_id,
        ReturnMailKind::Resolved(None),
      )
      .await?;
    }
    AdminReturnAction::RefundOrder { restock } => {
      let provider: String =
        sqlx::query_scalar("SELECT payment_provider FROM orders WHERE id = $1")
          .bind(order_id)
          .fetch_one(&mut **tx)
          .await?;
      if provider == crate::payments::ProviderKind::MercadoPago.as_str() {
        return Err(ReturnError::Invalid(
          "Pedido do Mercado Pago: use \"Estornar no Mercado Pago\".",
        ));
      }
      if !covers_whole_order(&mut **tx, return_id).await? {
        return Err(ReturnError::Invalid(
          "A solicitação não cobre o pedido inteiro: registre o reembolso parcial feito via PIX.",
        ));
      }
      // Locks the order and moves it to `refunded` (stock per the admin's
      // choice, coupon restored, order audit row + "reembolso" mail).
      apply_with_audit(
        tx,
        order_id,
        OrderEvent::Refund,
        ApplyOptions::with_restock(*restock),
        actor_id,
        actor_name,
      )
      .await?;
      let total: i64 =
        sqlx::query_scalar("SELECT total_cents FROM orders WHERE id = $1")
          .bind(order_id)
          .fetch_one(&mut **tx)
          .await?;
      metadata["restock"] = serde_json::json!(restock);
      metadata["refund_cents"] = serde_json::json!(total);
      resolve(tx, return_id, Resolution::Refunded, Some(total)).await?;
      crate::mail::returns::enqueue(
        tx,
        return_id,
        ReturnMailKind::Resolved(Some(total)),
      )
      .await?;
    }
    AdminReturnAction::RecordRefund {
      refund_cents,
      restock,
    } => {
      let (order_status, total_cents): (String, i64) = sqlx::query_as(
        "SELECT status::text, total_cents FROM orders WHERE id = $1 FOR UPDATE",
      )
      .bind(order_id)
      .fetch_one(&mut **tx)
      .await?;
      let order_refunded = order_status == "refunded";
      if !order_refunded && covers_whole_order(&mut **tx, return_id).await? {
        return Err(ReturnError::Invalid(
          "A solicitação cobre o pedido inteiro: use o estorno do pedido.",
        ));
      }
      let lines = returned_lines(tx, return_id).await?;
      let items_cents: i64 = lines
        .iter()
        .map(|(_, qty, price)| i64::from(*qty).saturating_mul(*price))
        .sum();
      let max = items_cents.min(total_cents);
      if *refund_cents <= 0 || *refund_cents > max {
        return Err(ReturnError::Invalid(
          "Valor do reembolso inválido (até o valor dos itens devolvidos).",
        ));
      }
      // An order refunded elsewhere already decided its stock.
      let restocked = *restock && !order_refunded;
      if restocked {
        let lines: Vec<ReserveLine> = lines
          .iter()
          .map(|(variant_id, quantity, _)| ReserveLine {
            variant_id: *variant_id,
            quantity: *quantity,
          })
          .collect();
        restock_lines(tx, order_id, &lines).await?;
      }
      metadata["restock"] = serde_json::json!(restocked);
      metadata["refund_cents"] = serde_json::json!(refund_cents);
      resolve(tx, return_id, Resolution::Refunded, Some(*refund_cents)).await?;
      crate::mail::returns::enqueue(
        tx,
        return_id,
        ReturnMailKind::Resolved(Some(*refund_cents)),
      )
      .await?;
    }
  }
  record(tx, return_id, actor_id, actor_name, metadata).await?;
  Ok(next)
}

async fn resolve(
  tx: &mut Transaction<'_, Postgres>,
  return_id: Uuid,
  resolution: Resolution,
  refund_cents: Option<i64>,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "UPDATE return_requests
     SET status = 'resolved', resolution = $2, refund_cents = $3,
         resolved_at = now(), updated_at = now()
     WHERE id = $1",
  )
  .bind(return_id)
  .bind(resolution.as_str())
  .bind(refund_cents)
  .execute(&mut **tx)
  .await?;
  Ok(())
}

/// Before calling Mercado Pago's refund for a request: it must be
/// `received`, refundable by kind, cover the whole order, and the order
/// must be an MP order. Returns the order id.
pub async fn check_mp_refund(
  pool: &PgPool,
  return_id: Uuid,
) -> Result<Uuid, ReturnError> {
  let mut tx = pool.begin().await?;
  let (order_id, kind, status) = lock_return(&mut tx, return_id).await?;
  tx.rollback().await?;
  let action = AdminReturnAction::RefundOrder { restock: false };
  if admin_transition(status, kind, &action).is_none() {
    return Err(ReturnError::IllegalTransition);
  }
  let options = refund_options(pool, return_id).await?;
  if !options.mercado_pago {
    return Err(ReturnError::Invalid("Este pedido não é do Mercado Pago."));
  }
  if !options.full_order {
    return Err(ReturnError::Invalid(
      "A solicitação não cobre o pedido inteiro: faça o reembolso parcial no painel do Mercado Pago e registre o valor.",
    ));
  }
  Ok(order_id)
}

/// After a successful Mercado Pago refund (the payments service already
/// moved the order to `refunded`, without restock from shipped/delivered):
/// optionally restock the order's units and resolve the request.
pub async fn finish_mp_refund(
  pool: &PgPool,
  return_id: Uuid,
  restock: bool,
  actor_id: Option<Uuid>,
  actor_name: Option<&str>,
) -> Result<(), ReturnError> {
  use crate::mail::returns::ReturnMailKind;

  let mut tx = pool.begin().await?;
  let (order_id, kind, from) = lock_return(&mut tx, return_id).await?;
  let action = AdminReturnAction::RefundOrder { restock };
  let next = admin_transition(from, kind, &action)
    .ok_or(ReturnError::IllegalTransition)?;
  let (order_status, total): (String, i64) = sqlx::query_as(
    "SELECT status::text, total_cents FROM orders WHERE id = $1 FOR UPDATE",
  )
  .bind(order_id)
  .fetch_one(&mut *tx)
  .await?;
  if order_status != "refunded" {
    return Err(ReturnError::Invalid(
      "O pedido ainda não consta como estornado. Use \"Sincronizar com Mercado Pago\" no pedido.",
    ));
  }
  if restock {
    restock_sold_items(&mut tx, order_id).await?;
  }
  resolve(&mut tx, return_id, Resolution::Refunded, Some(total)).await?;
  crate::mail::returns::enqueue(
    &mut tx,
    return_id,
    ReturnMailKind::Resolved(Some(total)),
  )
  .await?;
  record(
    &mut tx,
    return_id,
    actor_id,
    actor_name,
    serde_json::json!({
      "action": "mp_refund",
      "order_id": order_id,
      "from": from.as_str(),
      "to": next.as_str(),
      "restock": restock,
      "refund_cents": total,
    }),
  )
  .await?;
  tx.commit().await?;
  Ok(())
}

#[cfg(test)]
mod tests;
