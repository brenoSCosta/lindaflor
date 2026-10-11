const NAME_MIN: usize = 1;
const NAME_MAX: usize = 100;
const EMAIL_MAX: usize = 254;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
  Empty,
  TooLong,
}

impl std::fmt::Display for NameError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Empty => write!(f, "name is required"),
      Self::TooLong => write!(f, "name must be at most {NAME_MAX} characters"),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailError {
  Empty,
  TooLong,
  Invalid,
}

impl std::fmt::Display for EmailError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Empty | Self::Invalid => write!(f, "valid email is required"),
      Self::TooLong => {
        write!(f, "email must be at most {EMAIL_MAX} characters")
      }
    }
  }
}

/// Trimmed display name, 1..=100 Unicode chars.
pub fn validate_name(raw: &str) -> Result<String, NameError> {
  let name = raw.trim();
  let chars = name.chars().count();
  if chars < NAME_MIN {
    return Err(NameError::Empty);
  }
  if chars > NAME_MAX {
    return Err(NameError::TooLong);
  }
  Ok(name.to_string())
}

/// Trim + ASCII-lowercase email, max 254, one `@`, non-empty local/domain,
/// no spaces, domain has a dot.
pub fn validate_email(raw: &str) -> Result<String, EmailError> {
  let email = raw.trim().to_ascii_lowercase();
  if email.is_empty() {
    return Err(EmailError::Empty);
  }
  if email.chars().count() > EMAIL_MAX {
    return Err(EmailError::TooLong);
  }
  if email.contains(char::is_whitespace) || email.contains("..") {
    return Err(EmailError::Invalid);
  }
  let Some((local, domain)) = email.split_once('@') else {
    return Err(EmailError::Invalid);
  };
  if local.is_empty()
    || domain.is_empty()
    || local.starts_with('.')
    || local.ends_with('.')
    || domain.starts_with('.')
    || domain.ends_with('.')
    || !domain.contains('.')
    || email.matches('@').count() != 1
  {
    return Err(EmailError::Invalid);
  }
  if !local.chars().all(|c| {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '%' | '+' | '-')
  }) {
    return Err(EmailError::Invalid);
  }
  if !domain
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
  {
    return Err(EmailError::Invalid);
  }
  Ok(email)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn name_bounds() {
    assert!(validate_name("").is_err());
    assert!(validate_name("  ").is_err());
    assert_eq!(validate_name("  Ana  ").unwrap(), "Ana");
    let long: String = "a".repeat(101);
    assert!(validate_name(&long).is_err());
    assert!(validate_name(&"a".repeat(100)).is_ok());
  }

  #[test]
  fn email_rfc_ish() {
    assert!(validate_email("not-an-email").is_err());
    assert!(validate_email("a@b").is_err());
    assert!(validate_email("@x.com").is_err());
    assert!(validate_email("a@b.c").is_ok());
    assert_eq!(
      validate_email("  Foo.Bar+tag@Example.COM ").unwrap(),
      "foo.bar+tag@example.com"
    );
    let long_local = format!("{}@ex.com", "a".repeat(250));
    assert!(validate_email(&long_local).is_err());
  }
}
