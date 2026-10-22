//! Payment provider strategy (PRODUCTION_READINESS §2.0/§2.1).
//!
//! [`PaymentProvider`] is registered as app context next to the
//! `ObjectStore` (see `src/main.rs` / `src/app.rs`). Checkout, the order
//! page, the webhook, the admin order page and the expiry sweeper talk only
//! to this module plus `order.apply`; no page imports Mercado Pago HTTP
//! types.
//!
//! - `StaticPix`: the original locally generated BR Code (`store/pix.rs`)
//!   with manual "mark paid" in the admin. Dev / fallback only.
//! - `MercadoPago`: PIX charge via the MP Payments API, confirmed by a
//!   signed webhook that is re-verified against the API.

pub mod circuit;
pub mod config;
pub mod mercadopago;
pub mod service;
pub mod signature;
pub mod webhook;

#[cfg(test)]
pub(crate) mod test_mock;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use time::OffsetDateTime;
use topcoat::context::{Cx, try_app_context};
use uuid::Uuid;

pub use config::{PaymentConfig, ProviderKind};
pub use mercadopago::MercadoPagoClient;

/// Shown when the PSP is down or the circuit breaker is open.
pub const PAYMENT_UNAVAILABLE_MESSAGE: &str =
  "Pagamento temporariamente indisponível. Tente novamente em instantes.";

#[derive(Clone, Debug)]
pub enum PaymentProvider {
  StaticPix,
  MercadoPago(Arc<MercadoPagoClient>),
}

/// A new PIX charge. Every field comes from the persisted order, never
/// from the client.
#[derive(Debug, Clone)]
pub struct NewCharge {
  pub order_id: Uuid,
  pub amount_cents: i64,
  pub payer_email: String,
  pub expires_at: OffsetDateTime,
  pub description: String,
}

/// PSP status mapped into the few buckets `order.apply` cares about. The
/// raw MP string is stored for display only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeStatus {
  Pending,
  Approved,
  /// `rejected | cancelled | expired`: the charge can no longer be paid.
  Failed,
  /// `refunded | charged_back`: money went back to the payer.
  Refunded,
  /// Anything else (`in_process`, `in_mediation`, `authorized`, …): no
  /// order transition.
  Other,
}

impl ChargeStatus {
  pub fn from_mercadopago(raw: &str) -> Self {
    match raw {
      "pending" => Self::Pending,
      "approved" => Self::Approved,
      "rejected" | "cancelled" | "expired" => Self::Failed,
      "refunded" | "charged_back" => Self::Refunded,
      _ => Self::Other,
    }
  }
}

/// A created charge with its PIX instructions.
#[derive(Debug, Clone)]
pub struct Charge {
  pub payment_id: i64,
  pub raw_status: String,
  pub status_detail: Option<String>,
  pub qr_code: Option<String>,
  pub qr_code_base64: Option<String>,
  pub ticket_url: Option<String>,
}

/// Current state of a payment as reported by the PSP API.
#[derive(Debug, Clone)]
pub struct PaymentInfo {
  pub payment_id: i64,
  pub status: ChargeStatus,
  pub raw_status: String,
  pub status_detail: Option<String>,
  pub external_reference: Option<String>,
  pub amount_cents: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum PaymentError {
  /// Circuit breaker open: failed fast without calling the PSP.
  #[error("{}", PAYMENT_UNAVAILABLE_MESSAGE)]
  Unavailable,
  /// 5xx / timeout / connection failure after the retry budget.
  #[error("{}", PAYMENT_UNAVAILABLE_MESSAGE)]
  Transient(String),
  /// 4xx from the PSP (never retried).
  #[error("O Mercado Pago recusou a operação (HTTP {status}).")]
  Rejected { status: u16, message: String },
  /// 2xx with a body we could not understand.
  #[error("Resposta inválida do Mercado Pago.")]
  Decode(String),
  /// The configured provider cannot perform this operation.
  #[error("Operação indisponível para este meio de pagamento.")]
  NotSupported,
}

impl PaymentError {
  pub fn is_transient(&self) -> bool {
    matches!(self, Self::Unavailable | Self::Transient(_))
  }
}

impl PaymentProvider {
  pub fn from_config(cfg: &PaymentConfig) -> Self {
    match (cfg.kind, cfg.mercadopago.as_ref()) {
      (ProviderKind::MercadoPago, Some(mp)) => {
        Self::MercadoPago(Arc::new(MercadoPagoClient::new(mp.clone())))
      }
      _ => Self::StaticPix,
    }
  }

  /// The registered provider, or [`PaymentProvider::StaticPix`] when none
  /// was set (e.g. test routers).
  pub fn from_cx(cx: &Cx) -> Self {
    try_app_context::<PaymentProvider>(cx)
      .cloned()
      .unwrap_or(Self::StaticPix)
  }

  pub fn kind(&self) -> ProviderKind {
    match self {
      Self::StaticPix => ProviderKind::StaticPix,
      Self::MercadoPago(_) => ProviderKind::MercadoPago,
    }
  }

  pub fn mercadopago(&self) -> Option<&MercadoPagoClient> {
    match self {
      Self::MercadoPago(client) => Some(client),
      Self::StaticPix => None,
    }
  }

  /// False while the circuit breaker is open: checkout fails fast with
  /// [`PAYMENT_UNAVAILABLE_MESSAGE`] instead of creating an order that
  /// cannot get a charge.
  pub fn accepting_charges(&self) -> bool {
    match self {
      Self::StaticPix => true,
      Self::MercadoPago(client) => !client.circuit_open(),
    }
  }

  pub async fn create_charge(
    &self,
    charge: &NewCharge,
  ) -> Result<Charge, PaymentError> {
    match self {
      Self::StaticPix => Err(PaymentError::NotSupported),
      Self::MercadoPago(client) => client.create_pix_payment(charge).await,
    }
  }

  pub async fn fetch_status(
    &self,
    payment_id: i64,
  ) -> Result<PaymentInfo, PaymentError> {
    match self {
      Self::StaticPix => Err(PaymentError::NotSupported),
      Self::MercadoPago(client) => client.get_payment(payment_id).await,
    }
  }

  pub async fn refund(&self, payment_id: i64) -> Result<(), PaymentError> {
    match self {
      Self::StaticPix => Err(PaymentError::NotSupported),
      Self::MercadoPago(client) => client.refund_payment(payment_id).await,
    }
  }
}
