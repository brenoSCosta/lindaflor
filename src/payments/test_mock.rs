//! Mock Mercado Pago HTTP server for tests: a tiny HTTP/1.1 responder on
//! 127.0.0.1 whose base URL is injected through `MercadoPagoConfig::base_url`.
//! Implements `POST /v1/payments` (idempotent per `X-Idempotency-Key`),
//! `GET /v1/payments/{id}` and `POST /v1/payments/{id}/refunds`, records
//! every request, and can force status codes on the next requests.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::Rng;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::PaymentProvider;
use super::circuit::CircuitBreaker;
use super::config::MercadoPagoConfig;
use super::mercadopago::{MercadoPagoClient, RetryPolicy};

pub(crate) const SECRET: &str = "mock-webhook-secret";

#[derive(Debug, Clone)]
pub(crate) struct Recorded {
  pub method: String,
  pub path: String,
  pub idempotency_key: Option<String>,
  pub body: serde_json::Value,
}

struct MockState {
  payments: HashMap<i64, serde_json::Value>,
  by_key: HashMap<String, i64>,
  /// Status codes forced on the next requests, in order.
  forced: VecDeque<u16>,
  requests: Vec<Recorded>,
  next_id: i64,
}

pub(crate) struct MockMp {
  base_url: String,
  state: Arc<Mutex<MockState>>,
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
  buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

async fn serve_one(
  socket: &mut TcpStream,
  state: &Mutex<MockState>,
) -> std::io::Result<()> {
  let mut buf = Vec::new();
  let mut chunk = [0u8; 4096];
  let header_end = loop {
    let n = socket.read(&mut chunk).await?;
    if n == 0 {
      return Ok(());
    }
    buf.extend_from_slice(&chunk[..n]);
    if let Some(end) = find_header_end(&buf) {
      break end;
    }
  };
  let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
  let mut lines = head.split("\r\n");
  let mut request_line = lines.next().unwrap_or_default().split_whitespace();
  let method = request_line.next().unwrap_or_default().to_string();
  let path = request_line.next().unwrap_or_default().to_string();
  let mut content_length = 0usize;
  let mut idempotency_key = None;
  for line in lines {
    if let Some((name, value)) = line.split_once(':') {
      match name.trim().to_ascii_lowercase().as_str() {
        "content-length" => content_length = value.trim().parse().unwrap_or(0),
        "x-idempotency-key" => idempotency_key = Some(value.trim().to_string()),
        _ => {}
      }
    }
  }
  while buf.len() < header_end + content_length {
    let n = socket.read(&mut chunk).await?;
    if n == 0 {
      break;
    }
    buf.extend_from_slice(&chunk[..n]);
  }
  let end = (header_end + content_length).min(buf.len());
  let body: serde_json::Value =
    serde_json::from_slice(&buf[header_end..end]).unwrap_or_default();
  let (status, response) = respond(
    state,
    Recorded {
      method,
      path,
      idempotency_key,
      body,
    },
  );
  let payload = response.to_string();
  let reply = format!(
    "HTTP/1.1 {status} MOCK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
    payload.len()
  );
  socket.write_all(reply.as_bytes()).await?;
  socket.shutdown().await
}

fn respond(
  state: &Mutex<MockState>,
  request: Recorded,
) -> (u16, serde_json::Value) {
  let mut st = state.lock().unwrap();
  st.requests.push(request.clone());
  if let Some(code) = st.forced.pop_front() {
    return (
      code,
      serde_json::json!({ "message": "forced", "status": code }),
    );
  }
  let path = request.path.as_str();
  match request.method.as_str() {
    "POST" if path == "/v1/payments" => {
      if let Some(key) = &request.idempotency_key
        && let Some(id) = st.by_key.get(key)
      {
        return (200, st.payments[id].clone());
      }
      let id = st.next_id;
      st.next_id += 1;
      let payment = serde_json::json!({
        "id": id,
        "status": "pending",
        "status_detail": "pending_waiting_transfer",
        "external_reference": request.body["external_reference"],
        "transaction_amount": request.body["transaction_amount"],
        "point_of_interaction": {
          "transaction_data": {
            "qr_code": format!("00020126MOCKPIX{id}"),
            "qr_code_base64": "iVBORw0KGgo=",
            "ticket_url": format!("https://mp.test/ticket/{id}"),
          }
        }
      });
      st.payments.insert(id, payment.clone());
      if let Some(key) = request.idempotency_key {
        st.by_key.insert(key, id);
      }
      (201, payment)
    }
    "POST" if path.ends_with("/refunds") => {
      let id: i64 = path
        .trim_start_matches("/v1/payments/")
        .trim_end_matches("/refunds")
        .parse()
        .unwrap_or_default();
      match st.payments.get_mut(&id) {
        Some(payment) => {
          payment["status"] = serde_json::json!("refunded");
          (201, serde_json::json!({ "id": 1, "payment_id": id }))
        }
        None => (404, serde_json::json!({ "message": "not found" })),
      }
    }
    "GET" if path.starts_with("/v1/payments/") => {
      let id: i64 = path
        .trim_start_matches("/v1/payments/")
        .parse()
        .unwrap_or_default();
      match st.payments.get(&id) {
        Some(payment) => (200, payment.clone()),
        None => (404, serde_json::json!({ "message": "not found" })),
      }
    }
    _ => (404, serde_json::json!({ "message": "no route" })),
  }
}

impl MockMp {
  pub(crate) async fn start() -> Self {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // Unique ids per mock: `orders.mp_payment_id` is unique across the
    // shared test database.
    let next_id =
      rand::thread_rng().gen_range(1_000_000_000..9_000_000_000_000);
    let state = Arc::new(Mutex::new(MockState {
      payments: HashMap::new(),
      by_key: HashMap::new(),
      forced: VecDeque::new(),
      requests: Vec::new(),
      next_id,
    }));
    let accept_state = state.clone();
    tokio::spawn(async move {
      while let Ok((mut socket, _)) = listener.accept().await {
        let state = accept_state.clone();
        tokio::spawn(async move {
          let _ = serve_one(&mut socket, &state).await;
        });
      }
    });
    Self {
      base_url: format!("http://{addr}"),
      state,
    }
  }

  pub(crate) fn force(&self, codes: &[u16]) {
    self
      .state
      .lock()
      .unwrap()
      .forced
      .extend(codes.iter().copied());
  }

  pub(crate) fn set_field(
    &self,
    payment_id: i64,
    field: &str,
    value: serde_json::Value,
  ) {
    let mut st = self.state.lock().unwrap();
    st.payments.get_mut(&payment_id).expect("mock payment")[field] = value;
  }

  pub(crate) fn set_status(&self, payment_id: i64, status: &str) {
    self.set_field(payment_id, "status", serde_json::json!(status));
  }

  pub(crate) fn requests(&self) -> Vec<Recorded> {
    self.state.lock().unwrap().requests.clone()
  }

  pub(crate) fn count(&self, method: &str, path_prefix: &str) -> usize {
    self
      .requests()
      .iter()
      .filter(|r| r.method == method && r.path.starts_with(path_prefix))
      .count()
  }

  pub(crate) fn provider_with(
    &self,
    breaker: CircuitBreaker,
  ) -> PaymentProvider {
    PaymentProvider::MercadoPago(Arc::new(MercadoPagoClient::with_policy(
      MercadoPagoConfig {
        access_token: "TEST-access-token".to_string(),
        webhook_secret: SECRET.to_string(),
        base_url: self.base_url.clone(),
        notification_url: None,
      },
      RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_jitter: Duration::from_millis(1),
      },
      breaker,
    )))
  }

  pub(crate) fn provider(&self) -> PaymentProvider {
    self.provider_with(CircuitBreaker::new(5, Duration::from_secs(30)))
  }
}
