//! Transactional email.
//!
//! Callers never talk to SMTP on the request path: they render an
//! [`Email`] from [`templates`] and [`outbox::enqueue`] it — inside their
//! own transaction when they have one — and the background worker
//! ([`outbox::run_worker`], spawned from `main.rs`) delivers it through the
//! configured [`Mailer`] with retries and exponential backoff.

pub mod auth;
pub mod newsletter;
pub mod orders;
pub mod outbox;
pub mod templates;

use std::time::Duration;

use lettre::message::{Mailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::{MailConfig, MailTransport, SmtpTls};

pub use templates::Email;

/// Fallback origin for absolute links when `APP_ORIGIN` is unset (dev/test
/// only — production boot requires `APP_ORIGIN`).
const DEV_ORIGIN: &str = "http://localhost:4200";

const SMTP_TIMEOUT: Duration = Duration::from_secs(20);

/// Delivery strategy, registered as app context next to storage/valkey.
#[derive(Clone)]
pub enum Mailer {
  /// Log a one-line summary instead of sending (dev/test default).
  Log,
  Smtp(Box<SmtpMailer>),
}

#[derive(Clone)]
pub struct SmtpMailer {
  transport: AsyncSmtpTransport<Tokio1Executor>,
  from: Mailbox,
}

#[derive(Debug, thiserror::Error)]
pub enum MailError {
  #[error("invalid address: {0}")]
  Address(String),
  #[error("could not build message: {0}")]
  Build(String),
  #[error("smtp: {0}")]
  Smtp(String),
}

impl Mailer {
  pub fn from_config(config: &MailConfig) -> Result<Self, MailError> {
    let smtp = match &config.transport {
      MailTransport::Log => return Ok(Self::Log),
      MailTransport::Smtp(smtp) => smtp,
    };
    let from: Mailbox =
      config
        .from
        .parse()
        .map_err(|e: lettre::address::AddressError| {
          MailError::Address(format!("MAIL_FROM: {e}"))
        })?;
    let builder = match smtp.tls {
      SmtpTls::StartTls => {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)
          .map_err(|e| MailError::Smtp(e.to_string()))?
      }
      SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)
        .map_err(|e| MailError::Smtp(e.to_string()))?,
      SmtpTls::None => {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp.host)
      }
    };
    let mut builder = builder.port(smtp.port).timeout(Some(SMTP_TIMEOUT));
    if let Some(username) = &smtp.username {
      builder = builder.credentials(Credentials::new(
        username.clone(),
        smtp.password.clone().unwrap_or_default(),
      ));
    }
    Ok(Self::Smtp(Box::new(SmtpMailer {
      transport: builder.build(),
      from,
    })))
  }

  pub fn kind(&self) -> &'static str {
    match self {
      Self::Log => "log",
      Self::Smtp(_) => "smtp",
    }
  }

  /// Deliver one message. Called by the outbox worker only. With
  /// `unsubscribe_url` (marketing mail), the message carries
  /// `List-Unsubscribe` + `List-Unsubscribe-Post: List-Unsubscribe=One-Click`
  /// (RFC 8058).
  pub async fn send(
    &self,
    to: &str,
    email: &Email,
    unsubscribe_url: Option<&str>,
  ) -> Result<(), MailError> {
    match self {
      Self::Log => {
        // Bodies carry one-time links: never at info, never in production.
        tracing::info!(
          to = %redact_address(to),
          subject = %email.subject,
          list_unsubscribe = unsubscribe_url.is_some(),
          "mail (log transport, not sent)"
        );
        if crate::config::app_env() != "production" {
          tracing::debug!(to = %to, body = %email.text, "mail body");
        }
        Ok(())
      }
      Self::Smtp(smtp) => {
        let to: Mailbox =
          to.parse().map_err(|e: lettre::address::AddressError| {
            MailError::Address(e.to_string())
          })?;
        let mut builder = Message::builder()
          .from(smtp.from.clone())
          .to(to)
          .subject(email.subject.clone());
        for header in list_unsubscribe_headers(unsubscribe_url) {
          builder = builder.raw_header(header);
        }
        let message = builder
          .multipart(MultiPart::alternative_plain_html(
            email.text.clone(),
            email.html.clone(),
          ))
          .map_err(|e| MailError::Build(e.to_string()))?;
        smtp
          .transport
          .send(message)
          .await
          .map_err(|e| MailError::Smtp(e.to_string()))?;
        Ok(())
      }
    }
  }
}

/// RFC 2369 / RFC 8058 one-click unsubscribe headers for `url` (none
/// without one). The URL is ASCII (the address is percent-encoded).
fn list_unsubscribe_headers(
  url: Option<&str>,
) -> Vec<lettre::message::header::HeaderValue> {
  use lettre::message::header::{HeaderName, HeaderValue};
  let Some(url) = url else {
    return Vec::new();
  };
  vec![
    HeaderValue::new(
      HeaderName::new_from_ascii_str("List-Unsubscribe"),
      format!("<{url}>"),
    ),
    HeaderValue::new(
      HeaderName::new_from_ascii_str("List-Unsubscribe-Post"),
      "List-Unsubscribe=One-Click".to_string(),
    ),
  ]
}

/// `ana@example.com` → `a***@example.com`, for logs.
pub fn redact_address(address: &str) -> String {
  match address.split_once('@') {
    Some((local, domain)) => {
      let first = local.chars().next().unwrap_or('*');
      format!("{first}***@{domain}")
    }
    None => "***".to_string(),
  }
}

/// Absolute URL for `path` (which must start with `/`) on `APP_ORIGIN`.
pub fn absolute_url(path: &str) -> String {
  let origin =
    crate::auth::google::app_origin().unwrap_or_else(|| DEV_ORIGIN.to_string());
  format!("{origin}{path}")
}

/// Percent-encode a query value (tokens are URL-safe today; this keeps
/// links valid if that ever changes).
pub fn encode_query_value(value: &str) -> String {
  let mut out = String::with_capacity(value.len());
  for byte in value.bytes() {
    if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')
    {
      out.push(byte as char);
    } else {
      out.push_str(&format!("%{byte:02X}"));
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn redact_address_hides_local_part() {
    assert_eq!(redact_address("ana@example.com"), "a***@example.com");
    assert_eq!(redact_address("nope"), "***");
  }

  #[test]
  fn encode_query_value_escapes_reserved() {
    assert_eq!(encode_query_value("abc-_.~"), "abc-_.~");
    assert_eq!(encode_query_value("a b&c=d"), "a%20b%26c%3Dd");
  }

  #[tokio::test]
  async fn log_mailer_never_fails() {
    let email = templates::password_reset("Ana", "http://x/reset?token=t");
    Mailer::Log
      .send("ana@example.com", &email, None)
      .await
      .expect("log transport");
  }

  #[test]
  fn list_unsubscribe_headers_are_one_click() {
    assert!(list_unsubscribe_headers(None).is_empty());
    let url =
      "https://loja.example.com/newsletter/cancelar?email=a%40b.com&token=ff";
    let headers = list_unsubscribe_headers(Some(url));
    let message = headers
      .into_iter()
      .fold(
        Message::builder()
          .from("Loja <loja@example.com>".parse().unwrap())
          .to("ana@example.com".parse().unwrap())
          .subject("x"),
        |builder, header| builder.raw_header(header),
      )
      .body("oi".to_string())
      .unwrap();
    let raw = String::from_utf8(message.formatted()).unwrap();
    assert!(raw.contains(&format!("List-Unsubscribe: <{url}>")), "{raw}");
    assert!(raw.contains("List-Unsubscribe-Post: List-Unsubscribe=One-Click"));
  }

  // The pooled transport spawns its reaper on the Tokio runtime.
  #[tokio::test]
  async fn smtp_mailer_builds_from_config() {
    let config = MailConfig {
      transport: MailTransport::Smtp(crate::config::SmtpConfig {
        host: "127.0.0.1".to_string(),
        port: 1025,
        tls: SmtpTls::None,
        username: None,
        password: None,
      }),
      from: "Lindaflor <loja@example.com>".to_string(),
    };
    let mailer = Mailer::from_config(&config).expect("smtp mailer");
    assert_eq!(mailer.kind(), "smtp");
    let bad = MailConfig {
      from: "not an address".to_string(),
      ..config
    };
    assert!(matches!(
      Mailer::from_config(&bad),
      Err(MailError::Address(_))
    ));
  }
}
