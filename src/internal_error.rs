use std::fmt::Debug;

use topcoat::Error;
use topcoat::context::Cx;
use uuid::Uuid;

use crate::logging::request_store::get_request_store;

/// Generic 500: log the real error, return only `internal error ({error_id})`.
pub fn internal_error(err: &dyn Debug) -> Error {
  let error_id = Uuid::now_v7();
  tracing::error!(error_id = %error_id, ?err, "internal error");
  Error::msg(format!("internal error ({error_id})"))
}

/// Same as [`internal_error`], reusing the request id when logging is on.
pub fn internal_error_cx(cx: &Cx, err: &dyn Debug) -> Error {
  let error_id = get_request_store(cx)
    .map(|store| store.request_id)
    .unwrap_or_else(Uuid::now_v7);
  tracing::error!(error_id = %error_id, ?err, "internal error");
  Error::msg(format!("internal error ({error_id})"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hides_sqlx_text() {
    let err = internal_error(
      &"error returned from database: relation \"orders\" does not exist",
    );
    let msg = err.to_string();
    assert!(msg.starts_with("internal error ("), "{msg}");
    assert!(!msg.contains("relation"));
    assert!(!msg.contains("orders"));
  }
}
