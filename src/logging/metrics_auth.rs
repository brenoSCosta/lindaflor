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

pub fn metrics_request_is_authorized(
  authorization: Option<&str>,
  token: Option<&str>,
) -> bool {
  match token {
    None => true,
    Some("") => true,
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
    assert!(metrics_request_is_authorized(None, None));
    assert!(metrics_request_is_authorized(None, Some("")));
    assert!(metrics_request_is_authorized(Some("Bearer x"), None));
  }

  #[test]
  fn wrong_token_is_rejected() {
    assert!(!metrics_request_is_authorized(
      Some("Bearer other"),
      Some("secret")
    ));
  }

  #[test]
  fn good_bearer_is_accepted() {
    assert!(metrics_request_is_authorized(
      Some("Bearer secret"),
      Some("secret")
    ));
    assert!(metrics_request_is_authorized(
      Some("Bearer  secret  "),
      Some("secret")
    ));
  }

  #[test]
  fn missing_header_is_rejected_when_token_set() {
    assert!(!metrics_request_is_authorized(None, Some("secret")));
    assert!(!metrics_request_is_authorized(
      Some("secret"),
      Some("secret")
    ));
  }
}
