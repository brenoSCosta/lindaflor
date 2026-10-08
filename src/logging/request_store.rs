use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use topcoat::context::{Cx, try_request_context};
use uuid::Uuid;

use super::log_entry::RequestError;

#[derive(Debug, Clone)]
pub struct RequestUser {
  pub user_id: String,
  pub session_id: String,
  pub role: Option<String>,
}

#[derive(Debug)]
pub struct RequestStore {
  pub request_id: Uuid,
  pub start_time_epoch_ms: i64,
  pub start: Instant,
  pub route: String,
  inner: Mutex<RequestStoreInner>,
}

#[derive(Debug, Default)]
struct RequestStoreInner {
  user: Option<RequestUser>,
  error: Option<RequestError>,
  logged: bool,
}

impl RequestStore {
  pub fn new(
    request_id: Uuid,
    start_time_epoch_ms: i64,
    start: Instant,
    route: String,
  ) -> Arc<Self> {
    Arc::new(Self {
      request_id,
      start_time_epoch_ms,
      start,
      route,
      inner: Mutex::new(RequestStoreInner::default()),
    })
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, RequestStoreInner> {
    self.inner.lock().unwrap_or_else(PoisonError::into_inner)
  }

  pub fn mark_logged(&self) -> bool {
    let mut inner = self.lock();
    if inner.logged {
      return false;
    }
    inner.logged = true;
    true
  }

  pub fn user(&self) -> Option<RequestUser> {
    self.lock().user.clone()
  }

  pub fn error(&self) -> Option<RequestError> {
    self.lock().error.clone()
  }

  pub fn set_user(&self, user: RequestUser) {
    self.lock().user = Some(user);
  }

  pub fn set_error(&self, error: RequestError) {
    self.lock().error = Some(error);
  }
}

pub fn get_request_store(cx: &Cx) -> Option<&Arc<RequestStore>> {
  try_request_context::<Arc<RequestStore>>(cx)
}

pub fn set_request_user(cx: &Cx, user: RequestUser) {
  if let Some(store) = get_request_store(cx) {
    store.set_user(user);
  }
}

pub fn set_request_error(cx: &Cx, error: RequestError) {
  if let Some(store) = get_request_store(cx) {
    store.set_error(error);
  }
}
