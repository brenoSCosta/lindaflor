//! `POST /api/webhooks/mercadopago` — MP payment notifications.
//!
//! Cross-origin server-to-server POST: no cookies/session are used, so it
//! is exempt from `OriginPolicy` (see `src/app.rs`) and authenticated by
//! the `x-signature` HMAC instead. The body is never trusted: only the
//! signed `data.id` is used, and the payment is re-fetched from the MP API
//! before `order.apply`. Redelivery is harmless (see `service.rs`).
//!
//! Response: 401 on a bad/stale signature; 200 once the payment is applied
//! (or still being applied after [`APPLY_BUDGET`] — MP expects an answer
//! within ~20s); 503 when MP itself could not be reached, so MP retries
//! the notification later.

use std::collections::HashMap;
use std::time::Duration;

use http::StatusCode;
use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Json,
    error::{not_found, unauthorized},
    parse_query_params, request, route,
  },
};

use super::PaymentProvider;
use super::service::{MERCADO_PAGO_ACTOR, ServiceError, sync_payment};
use super::signature;

/// Route path (also referenced by the origin-policy exemption and the
/// rate-limit scope in `src/app.rs`).
pub const WEBHOOK_PATH: &str = "/api/webhooks/mercadopago";

/// How long the handler waits for fetch + apply before answering 200 and
/// letting the work finish in the background.
pub const APPLY_BUDGET: Duration = Duration::from_secs(8);

#[derive(Debug, Default, Deserialize)]
struct NotificationBody {
  #[serde(default, rename = "type")]
  kind: Option<String>,
}

fn now_secs() -> i64 {
  time::OffsetDateTime::now_utc().unix_timestamp()
}

fn reply(
  status: StatusCode,
  label: &str,
) -> (StatusCode, Json<serde_json::Value>) {
  (status, Json(serde_json::json!({ "status": label })))
}

#[route(POST "/api/webhooks/mercadopago")]
pub async fn mercadopago_webhook(
  cx: &Cx,
  body: String,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
  let provider = PaymentProvider::from_cx(cx);
  let Some(client) = provider.mercadopago() else {
    return Err(not_found().into());
  };

  let query: HashMap<String, String> =
    parse_query_params(cx).unwrap_or_default();
  let data_id = query.get("data.id").map(|id| id.trim());
  let headers = request::headers(cx);
  let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());

  if let Err(error) = signature::verify(
    client.webhook_secret(),
    header("x-signature"),
    header("x-request-id"),
    data_id,
    now_secs(),
  ) {
    tracing::warn!(?error, "mercado pago webhook rejected");
    return Err(unauthorized().into());
  }

  let body: NotificationBody = serde_json::from_str(&body).unwrap_or_default();
  let kind = query.get("type").cloned().or(body.kind).unwrap_or_default();
  if kind != "payment" {
    return Ok(reply(StatusCode::OK, "ignored"));
  }
  let Some(payment_id) = data_id.and_then(|id| id.parse::<i64>().ok()) else {
    return Ok(reply(StatusCode::OK, "ignored"));
  };

  let pool = app_context::<PgPool>(cx).clone();
  let task = tokio::spawn(async move {
    sync_payment(&pool, &provider, payment_id, MERCADO_PAGO_ACTOR).await
  });
  match tokio::time::timeout(APPLY_BUDGET, task).await {
    Ok(Ok(Ok(outcome))) => {
      tracing::info!(
        mp_payment_id = payment_id,
        ?outcome,
        "mercado pago webhook"
      );
      Ok(reply(StatusCode::OK, "ok"))
    }
    Ok(Ok(Err(ServiceError::Payment(error)))) if error.is_transient() => {
      tracing::warn!(mp_payment_id = payment_id, %error, "mercado pago unreachable; asking for redelivery");
      Ok(reply(StatusCode::SERVICE_UNAVAILABLE, "retry"))
    }
    // 4xx from MP (e.g. the dashboard's test notification for a payment
    // that does not exist): redelivery cannot help.
    Ok(Ok(Err(ServiceError::Payment(error)))) => {
      tracing::warn!(mp_payment_id = payment_id, %error, "mercado pago webhook ignored");
      Ok(reply(StatusCode::OK, "ignored"))
    }
    Ok(Ok(Err(error))) => {
      tracing::error!(mp_payment_id = payment_id, %error, "mercado pago webhook failed");
      Ok(reply(StatusCode::SERVICE_UNAVAILABLE, "retry"))
    }
    Ok(Err(join_error)) => {
      tracing::error!(mp_payment_id = payment_id, %join_error, "mercado pago webhook task panicked");
      Ok(reply(StatusCode::SERVICE_UNAVAILABLE, "retry"))
    }
    // Still running: it completes in the background; acknowledge now.
    Err(_) => Ok(reply(StatusCode::OK, "accepted")),
  }
}
