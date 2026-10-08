use std::time::Instant;

use http::HeaderValue;
use topcoat::context::Cx;
use topcoat::router::{
  Body, Layer, LayerFuture, Next, Path, header, request,
  response::response_headers,
};
use uuid::Uuid;

use super::log_entry::{RequestError, RequestLogEntry, SampleReason};
use super::metrics::record_request;
use super::request_store::RequestStore;
use super::route_label::normalize_route_label;
use super::sample::{SampleDecisionInput, decide_sample};
use super::sink::{LogSink, create_log_sink};

pub const METRICS_PATH: &str = "/metrics";

pub struct RequestLoggingLayer {
  pub(crate) sample_rate: f64,
  pub(crate) slow_threshold_ms: u64,
  sink: LogSink,
}

impl RequestLoggingLayer {
  pub fn new(sample_rate: f64, slow_threshold_ms: u64, sink: LogSink) -> Self {
    Self {
      sample_rate,
      slow_threshold_ms,
      sink,
    }
  }

  pub fn from_env() -> Self {
    let file_path =
      crate::config::log_file_path().map(std::path::PathBuf::from);
    Self::new(
      crate::config::log_sample_rate(),
      crate::config::log_slow_threshold_ms(),
      create_log_sink(
        file_path,
        crate::config::log_file_max_bytes(),
        crate::config::log_file_max_files(),
      ),
    )
  }

  pub fn noop() -> Self {
    Self::new(1.0, 1000, std::sync::Arc::new(|_| {}))
  }
}

fn epoch_ms_now() -> i64 {
  let now = time::OffsetDateTime::now_utc();
  (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn rfc3339_from_epoch_ms(ms: i64) -> String {
  let nanos = i128::from(ms).saturating_mul(1_000_000);
  let dt = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
    .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
  format!(
    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
    dt.year(),
    u8::from(dt.month()),
    dt.day(),
    dt.hour(),
    dt.minute(),
    dt.second(),
    dt.millisecond(),
  )
}

fn truncate_ua(ua: String) -> String {
  if ua.len() <= 256 {
    return ua;
  }
  let mut end = 256;
  while end > 0 && !ua.is_char_boundary(end) {
    end -= 1;
  }
  ua[..end].to_string()
}

fn status_from_error(error: &topcoat::Error) -> u16 {
  use topcoat::router::error::{
    BadRequestError, ContentTooLargeError, ForbiddenError, InternalServerError,
    MethodNotAllowedError, NotFoundError, RedirectError, SeeOther,
    ServiceUnavailableError, TooManyRequestsError, UnauthorizedError,
  };
  if error.downcast_ref::<NotFoundError>().is_some() {
    return 404;
  }
  if error.downcast_ref::<MethodNotAllowedError>().is_some() {
    return 405;
  }
  if error.downcast_ref::<BadRequestError>().is_some() {
    return 400;
  }
  if error.downcast_ref::<UnauthorizedError>().is_some() {
    return 401;
  }
  if error.downcast_ref::<ForbiddenError>().is_some() {
    return 403;
  }
  if error.downcast_ref::<TooManyRequestsError>().is_some() {
    return 429;
  }
  if error.downcast_ref::<ContentTooLargeError>().is_some() {
    return 413;
  }
  if error.downcast_ref::<ServiceUnavailableError>().is_some() {
    return 503;
  }
  if error.downcast_ref::<InternalServerError>().is_some() {
    return 500;
  }
  if error.downcast_ref::<SeeOther>().is_some() {
    return 303;
  }
  if error.downcast_ref::<RedirectError>().is_some() {
    return 307;
  }
  500
}

fn error_code_for_status(status: u16) -> &'static str {
  match status {
    400 => "BAD_REQUEST",
    401 => "UNAUTHORIZED",
    403 => "FORBIDDEN",
    404 => "NOT_FOUND",
    405 => "METHOD_NOT_ALLOWED",
    413 => "CONTENT_TOO_LARGE",
    429 => "TOO_MANY_REQUESTS",
    503 => "SERVICE_UNAVAILABLE",
    303 => "SEE_OTHER",
    307 | 308 => "REDIRECT",
    _ => "INTERNAL",
  }
}

fn client_safe_error_message(error: &topcoat::Error, status: u16) -> String {
  use topcoat::router::error::{
    BadRequestError, ContentTooLargeError, ForbiddenError,
    MethodNotAllowedError, NotFoundError, RedirectError, SeeOther,
    ServiceUnavailableError, TooManyRequestsError, UnauthorizedError,
  };
  if let Some(err) = error.downcast_ref::<NotFoundError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<MethodNotAllowedError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<BadRequestError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<UnauthorizedError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<ForbiddenError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<TooManyRequestsError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<ContentTooLargeError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<ServiceUnavailableError>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<SeeOther>() {
    return err.to_string();
  }
  if let Some(err) = error.downcast_ref::<RedirectError>() {
    return err.to_string();
  }
  let _ = status;
  "internal error".to_string()
}

fn append_request_id(cx: &Cx, request_id: Uuid) {
  if let Ok(value) = HeaderValue::from_str(&request_id.to_string()) {
    response_headers(cx)
      .append(header::HeaderName::from_static("x-request-id"), value);
  }
}

impl Layer for RequestLoggingLayer {
  fn path(&self) -> Option<&Path> {
    None
  }

  fn handle<'a>(
    &'a self,
    cx: &'a Cx,
    body: Body,
    next: Next<'a>,
  ) -> LayerFuture<'a> {
    Box::pin(async move {
      let method = request::method(cx).clone();
      let path = request::uri(cx).path().to_owned();
      if method == http::Method::OPTIONS || path == METRICS_PATH {
        return next.run(cx, body).await;
      }

      let request_id = Uuid::now_v7();
      let start = Instant::now();
      let start_time_epoch_ms = epoch_ms_now();
      let route = normalize_route_label(&path);
      let store = RequestStore::new(
        request_id,
        start_time_epoch_ms,
        start,
        route.clone(),
      );
      let child = cx.with(std::sync::Arc::clone(&store));
      append_request_id(cx, request_id);
      append_request_id(&child, request_id);

      let result = next.run(&child, body).await;
      let duration_ms = store.start.elapsed().as_millis() as u64;
      let status = match &result {
        Ok(resp) => resp.status().as_u16(),
        Err(err) => status_from_error(err),
      };

      if let Err(err) = &result
        && store.error().is_none()
      {
        store.set_error(RequestError {
          code: error_code_for_status(status).to_string(),
          message: client_safe_error_message(err, status),
        });
      }

      if store.mark_logged() {
        record_request(method.as_str(), &store.route, status, duration_ms);
        let reason = decide_sample(SampleDecisionInput {
          status,
          duration_ms,
          rate: self.sample_rate,
          slow_threshold_ms: self.slow_threshold_ms,
          random: rand::random::<f64>(),
        });
        if reason != SampleReason::Dropped {
          let user = store.user();
          let entry = RequestLogEntry {
            request_id: store.request_id,
            timestamp: rfc3339_from_epoch_ms(store.start_time_epoch_ms),
            method: method.as_str().to_string(),
            path,
            duration_ms,
            client_ip: crate::valkey::client_ip(&child),
            user_agent: truncate_ua(crate::valkey::client_ua(&child)),
            user_id: user.as_ref().map(|u| u.user_id.clone()),
            session_id: user.as_ref().map(|u| u.session_id.clone()),
            role: user.and_then(|u| u.role),
            status,
            error: store.error(),
            sampled: true,
            sample_reason: reason,
          };
          (self.sink)(entry);
        }
      }

      result
    })
  }
}
