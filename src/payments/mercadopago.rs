//! Mercado Pago Payments API adapter: MP JSON ↔ [`Charge`] / [`PaymentInfo`].
//!
//! Outbound calls share [`crate::http_client::shared_http_client`] (10s
//! timeout, pooled), retry only transient failures (5xx, timeout,
//! connection error) with exponential backoff + jitter up to
//! [`RetryPolicy::max_attempts`], and sit behind a [`CircuitBreaker`].
//! Payment creation carries `X-Idempotency-Key = order id`, so a retry (or
//! a second attempt from the order page) can never double-charge.

use std::time::Duration;

use rand::Rng;
use serde::Deserialize;
use time::{OffsetDateTime, UtcOffset};

use super::circuit::CircuitBreaker;
use super::config::MercadoPagoConfig;
use super::{Charge, ChargeStatus, NewCharge, PaymentError, PaymentInfo};

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
  pub max_attempts: u32,
  pub base_delay: Duration,
  pub max_jitter: Duration,
}

impl Default for RetryPolicy {
  fn default() -> Self {
    Self {
      max_attempts: 3,
      base_delay: Duration::from_millis(200),
      max_jitter: Duration::from_millis(150),
    }
  }
}

impl RetryPolicy {
  /// Delay before retry number `attempt` (1-based): `base * 2^(attempt-1)`
  /// plus uniform jitter.
  fn backoff(&self, attempt: u32) -> Duration {
    let exp = self
      .base_delay
      .saturating_mul(1u32 << attempt.saturating_sub(1).min(8));
    let jitter_ms = self.max_jitter.as_millis() as u64;
    let jitter = if jitter_ms == 0 {
      0
    } else {
      rand::thread_rng().gen_range(0..=jitter_ms)
    };
    exp + Duration::from_millis(jitter)
  }
}

pub struct MercadoPagoClient {
  http: reqwest::Client,
  config: MercadoPagoConfig,
  retry: RetryPolicy,
  breaker: CircuitBreaker,
}

// Manual `Debug`: the config's own `Debug` redacts the secrets.
impl std::fmt::Debug for MercadoPagoClient {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("MercadoPagoClient")
      .field("config", &self.config)
      .field("retry", &self.retry)
      .finish_non_exhaustive()
  }
}

/// Consecutive transient failures that open the breaker, and how long it
/// stays open before a half-open probe.
const BREAKER_THRESHOLD: u32 = 5;
const BREAKER_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct MpPayment {
  id: i64,
  status: String,
  status_detail: Option<String>,
  external_reference: Option<String>,
  transaction_amount: Option<f64>,
  point_of_interaction: Option<MpPointOfInteraction>,
}

#[derive(Deserialize)]
struct MpPointOfInteraction {
  transaction_data: Option<MpTransactionData>,
}

#[derive(Deserialize)]
struct MpTransactionData {
  qr_code: Option<String>,
  qr_code_base64: Option<String>,
  ticket_url: Option<String>,
}

#[derive(Deserialize)]
struct MpErrorBody {
  message: Option<String>,
}

/// MP's `date_of_expiration` format: `2026-10-24T12:00:00.000-03:00`.
/// Rendered in Brasília time (no DST since 2019).
pub fn format_mp_datetime(at: OffsetDateTime) -> String {
  let offset = UtcOffset::from_hms(-3, 0, 0).expect("valid offset");
  let local = at.to_offset(offset);
  format!(
    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}-03:00",
    local.year(),
    u8::from(local.month()),
    local.day(),
    local.hour(),
    local.minute(),
    local.second(),
    local.millisecond(),
  )
}

/// `R$ 123,45` → MP's decimal `transaction_amount` (`123.45`).
fn cents_to_amount(cents: i64) -> serde_json::Value {
  serde_json::Number::from_f64(cents as f64 / 100.0)
    .map(serde_json::Value::Number)
    .unwrap_or(serde_json::Value::Null)
}

fn amount_to_cents(amount: f64) -> i64 {
  (amount * 100.0).round() as i64
}

impl MercadoPagoClient {
  pub fn new(config: MercadoPagoConfig) -> Self {
    Self::with_policy(
      config,
      RetryPolicy::default(),
      CircuitBreaker::new(BREAKER_THRESHOLD, BREAKER_COOLDOWN),
    )
  }

  pub fn with_policy(
    config: MercadoPagoConfig,
    retry: RetryPolicy,
    breaker: CircuitBreaker,
  ) -> Self {
    Self {
      http: crate::http_client::shared_http_client().clone(),
      config,
      retry,
      breaker,
    }
  }

  pub fn webhook_secret(&self) -> &str {
    &self.config.webhook_secret
  }

  pub fn circuit_open(&self) -> bool {
    self.breaker.is_open()
  }

  fn url(&self, path: &str) -> String {
    format!("{}{}", self.config.base_url, path)
  }

  /// Send with retry + breaker. `build` is called once per attempt so each
  /// retry sends an identical request (same idempotency key).
  async fn send_json<T, F>(&self, build: F) -> Result<T, PaymentError>
  where
    T: serde::de::DeserializeOwned,
    F: Fn() -> reqwest::RequestBuilder,
  {
    if !self.breaker.allow() {
      return Err(PaymentError::Unavailable);
    }
    let attempts = self.retry.max_attempts.max(1);
    let mut last_error = String::new();
    for attempt in 0..attempts {
      if attempt > 0 {
        tokio::time::sleep(self.retry.backoff(attempt)).await;
      }
      match build().bearer_auth(&self.config.access_token).send().await {
        Ok(response) if response.status().is_server_error() => {
          last_error = format!("HTTP {}", response.status().as_u16());
        }
        Ok(response) if response.status().is_success() => {
          return match response.json::<T>().await {
            Ok(value) => {
              self.breaker.on_success();
              Ok(value)
            }
            Err(error) => {
              self.breaker.on_neutral();
              Err(PaymentError::Decode(error.to_string()))
            }
          };
        }
        Ok(response) => {
          // 4xx: our request is wrong (bad token, invalid payer, …).
          // Retrying cannot help and it says nothing about MP's health.
          let status = response.status().as_u16();
          let message = response
            .json::<MpErrorBody>()
            .await
            .ok()
            .and_then(|body| body.message)
            .unwrap_or_default();
          self.breaker.on_neutral();
          return Err(PaymentError::Rejected { status, message });
        }
        Err(error) if error.is_timeout() || error.is_connect() => {
          last_error = if error.is_timeout() {
            "timeout".to_string()
          } else {
            "connection failed".to_string()
          };
        }
        Err(error) => {
          self.breaker.on_failure();
          return Err(PaymentError::Transient(error.to_string()));
        }
      }
      tracing::warn!(
        attempt = attempt + 1,
        error = %last_error,
        "mercado pago request failed"
      );
    }
    self.breaker.on_failure();
    Err(PaymentError::Transient(last_error))
  }

  /// `POST /v1/payments` with `payment_method_id = "pix"`.
  pub async fn create_pix_payment(
    &self,
    charge: &NewCharge,
  ) -> Result<Charge, PaymentError> {
    let mut body = serde_json::json!({
      "transaction_amount": cents_to_amount(charge.amount_cents),
      "description": charge.description,
      "payment_method_id": "pix",
      "payer": { "email": charge.payer_email },
      "external_reference": charge.order_id.to_string(),
      "date_of_expiration": format_mp_datetime(charge.expires_at),
    });
    if let Some(url) = &self.config.notification_url {
      body["notification_url"] = serde_json::Value::String(url.clone());
    }
    let idempotency_key = charge.order_id.to_string();
    let payment: MpPayment = self
      .send_json(|| {
        self
          .http
          .post(self.url("/v1/payments"))
          .header("X-Idempotency-Key", idempotency_key.as_str())
          .json(&body)
      })
      .await?;
    let data = payment
      .point_of_interaction
      .and_then(|poi| poi.transaction_data);
    let (qr_code, qr_code_base64, ticket_url) = match data {
      Some(data) => (data.qr_code, data.qr_code_base64, data.ticket_url),
      None => (None, None, None),
    };
    Ok(Charge {
      payment_id: payment.id,
      raw_status: payment.status,
      status_detail: payment.status_detail,
      qr_code,
      qr_code_base64,
      ticket_url,
    })
  }

  /// `GET /v1/payments/{id}`: the source of truth for webhook handling.
  pub async fn get_payment(
    &self,
    payment_id: i64,
  ) -> Result<PaymentInfo, PaymentError> {
    let url = self.url(&format!("/v1/payments/{payment_id}"));
    let payment: MpPayment =
      self.send_json(|| self.http.get(url.as_str())).await?;
    Ok(PaymentInfo {
      payment_id: payment.id,
      status: ChargeStatus::from_mercadopago(&payment.status),
      raw_status: payment.status,
      status_detail: payment.status_detail,
      external_reference: payment.external_reference,
      amount_cents: payment.transaction_amount.map(amount_to_cents),
    })
  }

  /// `POST /v1/payments/{id}/refunds` (full refund). Idempotent per payment.
  pub async fn refund_payment(
    &self,
    payment_id: i64,
  ) -> Result<(), PaymentError> {
    let url = self.url(&format!("/v1/payments/{payment_id}/refunds"));
    let idempotency_key = format!("refund-{payment_id}");
    let _: serde_json::Value = self
      .send_json(|| {
        self
          .http
          .post(url.as_str())
          .header("X-Idempotency-Key", idempotency_key.as_str())
          .json(&serde_json::json!({}))
      })
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod unit_tests {
  use super::*;

  #[test]
  fn formats_expiration_in_brasilia_time() {
    let at = OffsetDateTime::from_unix_timestamp(1_793_016_000).unwrap();
    // 2026-10-26T12:00:00Z → 09:00 in UTC-3.
    assert_eq!(format_mp_datetime(at), "2026-10-26T09:00:00.000-03:00");
  }

  #[test]
  fn amount_conversions_round_trip() {
    assert_eq!(cents_to_amount(12_345), serde_json::json!(123.45));
    assert_eq!(cents_to_amount(10), serde_json::json!(0.1));
    assert_eq!(amount_to_cents(123.45), 12_345);
    assert_eq!(amount_to_cents(0.1 + 0.2), 30);
  }

  #[test]
  fn maps_mercadopago_statuses() {
    assert_eq!(
      ChargeStatus::from_mercadopago("approved"),
      ChargeStatus::Approved
    );
    for raw in ["rejected", "cancelled", "expired"] {
      assert_eq!(ChargeStatus::from_mercadopago(raw), ChargeStatus::Failed);
    }
    for raw in ["refunded", "charged_back"] {
      assert_eq!(ChargeStatus::from_mercadopago(raw), ChargeStatus::Refunded);
    }
    assert_eq!(
      ChargeStatus::from_mercadopago("in_mediation"),
      ChargeStatus::Other
    );
  }
}
