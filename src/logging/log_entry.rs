use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleReason {
  Error,
  Slow,
  Sampled,
  Dropped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestError {
  pub code: String,
  pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestLogEntry {
  pub request_id: Uuid,
  pub timestamp: String,
  pub method: String,
  pub path: String,
  pub duration_ms: u64,
  pub client_ip: String,
  pub user_agent: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub user_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub session_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub role: Option<String>,
  pub status: u16,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<RequestError>,
  pub sampled: bool,
  pub sample_reason: SampleReason,
}
