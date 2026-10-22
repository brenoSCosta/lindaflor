pub fn presented_bearer_token(authorization: Option<&str>) -> Option<&str> {
  let value = authorization?;
  let rest = value.strip_prefix("Bearer ")?;
  Some(rest.trim())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
  if a.len() != b.len() {
    return false;
  }
  let mut acc = 0u8;
  for (x, y) in a.iter().zip(b.iter()) {
    acc |= x ^ y;
  }
  acc == 0
}

/// `token` unset (or empty) means open metrics, except in production where
/// it means closed: `/metrics` profiles traffic per route, so a missing
/// `METRICS_TOKEN` must never expose it publicly (boot also refuses to
/// start without one, see `config::Config::from_env`).
pub fn metrics_request_is_authorized(
  authorization: Option<&str>,
  token: Option<&str>,
  production: bool,
) -> bool {
  match token {
    None | Some("") => !production,
    Some(expected) => {
      presented_bearer_token(authorization).is_some_and(|presented| {
        constant_time_eq(presented.as_bytes(), expected.as_bytes())
      })
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn unset_or_empty_token_is_open() {
    assert!(metrics_request_is_authorized(None, None, false));
    assert!(metrics_request_is_authorized(None, Some(""), false));
    assert!(metrics_request_is_authorized(Some("Bearer x"), None, false));
  }

  #[test]
  fn unset_or_empty_token_is_closed_in_production() {
    assert!(!metrics_request_is_authorized(None, None, true));
    assert!(!metrics_request_is_authorized(None, Some(""), true));
    assert!(!metrics_request_is_authorized(Some("Bearer x"), None, true));
    assert!(!metrics_request_is_authorized(
      Some("Bearer "),
      Some(""),
      true
    ));
    assert!(metrics_request_is_authorized(
      Some("Bearer secret"),
      Some("secret"),
      true
    ));
  }

  #[test]
  fn wrong_token_is_rejected() {
    assert!(!metrics_request_is_authorized(
      Some("Bearer other"),
      Some("secret"),
      false
    ));
  }

  #[test]
  fn good_bearer_is_accepted() {
    assert!(metrics_request_is_authorized(
      Some("Bearer secret"),
      Some("secret"),
      false
    ));
    assert!(metrics_request_is_authorized(
      Some("Bearer  secret  "),
      Some("secret"),
      false
    ));
  }

  #[test]
  fn missing_header_is_rejected_when_token_set() {
    assert!(!metrics_request_is_authorized(None, Some("secret"), false));
    assert!(!metrics_request_is_authorized(
      Some("secret"),
      Some("secret"),
      false
    ));
  }
}
