//! Postgres outbox (`email_outbox`, migration 0020) and its drain worker.
//!
//! [`enqueue`] takes any executor so callers can enqueue inside the same
//! transaction as the state change that triggers the mail: a rollback drops
//! the mail too. The worker claims due rows with `FOR UPDATE SKIP LOCKED`
//! and pushes their `next_attempt_at` out by a lease before sending, so
//! several replicas never deliver the same row concurrently. Failures retry
//! with exponential backoff up to `MAIL_MAX_ATTEMPTS`, then the row is
//! parked as `failed`. Bodies are blanked once a row is terminal: they can
//! embed one-time links that must not linger at rest.

use std::time::Duration;

use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::{Email, Mailer, redact_address};

/// Rows claimed per drain pass.
const BATCH: i64 = 20;
/// How long a claimed row stays invisible to other workers while sending.
const LEASE_SECS: i64 = 300;
const BACKOFF_BASE_SECS: i64 = 30;
const BACKOFF_MAX_SECS: i64 = 3600;
const MAX_ERROR_LEN: usize = 500;

/// Stable identifiers for `email_outbox.kind`.
pub mod kind {
  pub const VERIFY_EMAIL: &str = "verify_email";
  pub const PASSWORD_RESET: &str = "password_reset";
  pub const CHANGE_EMAIL: &str = "change_email";
  pub const DELETE_ACCOUNT: &str = "delete_account";
  pub const ORDER_CREATED: &str = "order_created";
  pub const PAYMENT_CONFIRMED: &str = "payment_confirmed";
  pub const ORDER_SHIPPED: &str = "order_shipped";
  pub const ORDER_CANCELLED: &str = "order_cancelled";
  pub const ORDER_EXPIRED: &str = "order_expired";
  pub const ORDER_REFUNDED: &str = "order_refunded";
  pub const NEWSLETTER_WELCOME: &str = "newsletter_welcome";

  /// Marketing mail: delivered with `List-Unsubscribe` /
  /// `List-Unsubscribe-Post` headers (RFC 8058) pointing at the signed
  /// unsubscribe link of the recipient.
  pub const NEWSLETTER: [&str; 1] = [NEWSLETTER_WELCOME];

  /// Auth mail anyone can trigger for an arbitrary address (sign-up /
  /// resend verification, password reset, change-email confirmation):
  /// capped per recipient by [`super::enqueue_throttled`].
  pub const RECIPIENT_THROTTLED: [&str; 3] =
    [VERIFY_EMAIL, PASSWORD_RESET, CHANGE_EMAIL];
}

/// At most this many [`kind::RECIPIENT_THROTTLED`] mails per recipient
/// address per [`RECIPIENT_WINDOW_SECS`].
pub const RECIPIENT_MAX_PER_WINDOW: i64 = 5;
pub const RECIPIENT_WINDOW_SECS: i64 = 3600;

/// Queue `email` for `to`. With `dedupe_key`, a second enqueue under the
/// same key is a silent no-op.
pub async fn enqueue<'e, E>(
  executor: E,
  kind: &str,
  to: &str,
  email: &Email,
  dedupe_key: Option<&str>,
) -> Result<(), sqlx::Error>
where
  E: PgExecutor<'e>,
{
  sqlx::query(
    "INSERT INTO email_outbox (id, kind, to_address, subject, html_body, text_body, dedupe_key)
     VALUES ($1, $2, $3, $4, $5, $6, $7)
     ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING",
  )
  .bind(Uuid::now_v7())
  .bind(kind)
  .bind(to.trim())
  .bind(&email.subject)
  .bind(&email.html)
  .bind(&email.text)
  .bind(dedupe_key)
  .execute(executor)
  .await?;
  tracing::debug!(kind, to = %redact_address(to), "mail enqueued");
  Ok(())
}

/// [`enqueue`] for the [`kind::RECIPIENT_THROTTLED`] kinds: silently drops
/// the mail (returns `false`) once the recipient already has
/// [`RECIPIENT_MAX_PER_WINDOW`] of them queued or sent in the last
/// [`RECIPIENT_WINDOW_SECS`], so the endpoints cannot be used to flood a
/// mailbox (or burn SMTP reputation) one rotating IP at a time. Callers
/// answer the client exactly as on success: the cap must not reveal
/// anything about the address.
///
/// The count and insert are one statement; two racing requests can still
/// both pass at the edge (soft cap, off by at most the concurrency).
pub async fn enqueue_throttled<'e, E>(
  executor: E,
  kind: &str,
  to: &str,
  email: &Email,
) -> Result<bool, sqlx::Error>
where
  E: PgExecutor<'e>,
{
  let to = to.trim();
  let inserted = sqlx::query(
    "INSERT INTO email_outbox (id, kind, to_address, subject, html_body, text_body)
     SELECT $1, $2, $3, $4, $5, $6
     WHERE (
       SELECT count(*) FROM email_outbox
       WHERE lower(to_address) = lower($3)
         AND kind = ANY($7)
         AND created_at > now() - make_interval(secs => $8)
     ) < $9",
  )
  .bind(Uuid::now_v7())
  .bind(kind)
  .bind(to)
  .bind(&email.subject)
  .bind(&email.html)
  .bind(&email.text)
  .bind(&kind::RECIPIENT_THROTTLED[..])
  .bind(RECIPIENT_WINDOW_SECS as f64)
  .bind(RECIPIENT_MAX_PER_WINDOW)
  .execute(executor)
  .await?
  .rows_affected()
    > 0;
  if inserted {
    tracing::debug!(kind, to = %redact_address(to), "mail enqueued");
  } else {
    tracing::info!(
      kind,
      to = %redact_address(to),
      max = RECIPIENT_MAX_PER_WINDOW,
      window_secs = RECIPIENT_WINDOW_SECS,
      "auth mail dropped: per-recipient limit reached"
    );
  }
  Ok(inserted)
}

/// Delay before retry number `attempts + 1`: 30s, 60s, 120s, … capped at 1h.
pub fn backoff_secs(attempts: i32) -> i64 {
  let exponent = attempts.saturating_sub(1).clamp(0, 16) as u32;
  BACKOFF_BASE_SECS
    .saturating_mul(1_i64 << exponent)
    .min(BACKOFF_MAX_SECS)
}

#[derive(sqlx::FromRow)]
struct Claimed {
  id: Uuid,
  kind: String,
  to_address: String,
  subject: String,
  html_body: String,
  text_body: String,
  attempts: i32,
}

/// Counts from one [`drain_once`] pass.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DrainReport {
  pub sent: usize,
  pub retried: usize,
  pub failed: usize,
}

/// Claim and deliver one batch of due rows.
pub async fn drain_once(
  pool: &PgPool,
  mailer: &Mailer,
  max_attempts: i32,
) -> Result<DrainReport, sqlx::Error> {
  let claimed: Vec<Claimed> = sqlx::query_as(
    "UPDATE email_outbox
     SET attempts = attempts + 1,
         next_attempt_at = now() + make_interval(secs => $2)
     WHERE id IN (
       SELECT id FROM email_outbox
       WHERE status = 'pending' AND next_attempt_at <= now()
       ORDER BY next_attempt_at
       LIMIT $1
       FOR UPDATE SKIP LOCKED
     )
     RETURNING id, kind, to_address, subject, html_body, text_body, attempts",
  )
  .bind(BATCH)
  .bind(LEASE_SECS as f64)
  .fetch_all(pool)
  .await?;

  let mut report = DrainReport::default();
  for row in claimed {
    let email = Email {
      subject: row.subject,
      html: row.html_body,
      text: row.text_body,
    };
    let unsubscribe = kind::NEWSLETTER
      .contains(&row.kind.as_str())
      .then(|| crate::app::newsletter::unsubscribe_url(&row.to_address));
    match mailer
      .send(&row.to_address, &email, unsubscribe.as_deref())
      .await
    {
      Ok(()) => {
        sqlx::query(
          "UPDATE email_outbox
           SET status = 'sent', sent_at = now(), last_error = NULL,
               html_body = '', text_body = ''
           WHERE id = $1",
        )
        .bind(row.id)
        .execute(pool)
        .await?;
        report.sent += 1;
      }
      Err(error) => {
        let mut message = error.to_string();
        message.truncate(MAX_ERROR_LEN);
        if row.attempts >= max_attempts {
          sqlx::query(
            "UPDATE email_outbox
             SET status = 'failed', last_error = $2,
                 html_body = '', text_body = ''
             WHERE id = $1",
          )
          .bind(row.id)
          .bind(&message)
          .execute(pool)
          .await?;
          tracing::error!(
            id = %row.id,
            kind = %row.kind,
            to = %redact_address(&row.to_address),
            attempts = row.attempts,
            error = %message,
            "mail delivery failed permanently"
          );
          report.failed += 1;
        } else {
          sqlx::query(
            "UPDATE email_outbox
             SET last_error = $2,
                 next_attempt_at = now() + make_interval(secs => $3)
             WHERE id = $1",
          )
          .bind(row.id)
          .bind(&message)
          .bind(backoff_secs(row.attempts) as f64)
          .execute(pool)
          .await?;
          tracing::warn!(
            id = %row.id,
            kind = %row.kind,
            attempts = row.attempts,
            error = %message,
            "mail delivery failed; will retry"
          );
          report.retried += 1;
        }
      }
    }
  }
  Ok(report)
}

/// Background loop: drain every `MAIL_POLL_SECS`, draining back-to-back
/// while full batches keep coming. Never returns.
pub async fn run_worker(pool: PgPool, mailer: Mailer) {
  let poll = Duration::from_secs(crate::config::mail_poll_secs());
  let max_attempts = crate::config::mail_max_attempts();
  tracing::info!(transport = mailer.kind(), "mail outbox worker started");
  let mut interval = tokio::time::interval(poll);
  loop {
    interval.tick().await;
    loop {
      match drain_once(&pool, &mailer, max_attempts).await {
        Ok(report) => {
          let handled = report.sent + report.retried + report.failed;
          if handled < BATCH as usize {
            break;
          }
        }
        Err(error) => {
          tracing::error!(error = %error, "mail outbox drain failed");
          break;
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn backoff_grows_and_caps() {
    assert_eq!(backoff_secs(1), 30);
    assert_eq!(backoff_secs(2), 60);
    assert_eq!(backoff_secs(3), 120);
    assert_eq!(backoff_secs(10), BACKOFF_MAX_SECS);
    assert_eq!(backoff_secs(i32::MAX), BACKOFF_MAX_SECS);
  }

  fn sample() -> Email {
    Email {
      subject: "Assunto".to_string(),
      html: "<p>oi</p>".to_string(),
      text: "oi".to_string(),
    }
  }

  #[tokio::test]
  async fn enqueue_dedupes_and_drain_sends_and_scrubs() {
    let pool = crate::test_support::isolated_pool().await;
    let key = format!("test:{}", Uuid::now_v7());
    enqueue(&pool, "test", "ana@example.com", &sample(), Some(&key))
      .await
      .unwrap();
    enqueue(&pool, "test", "ana@example.com", &sample(), Some(&key))
      .await
      .unwrap();
    let count: i64 = sqlx::query_scalar(
      "SELECT count(*) FROM email_outbox WHERE dedupe_key = $1",
    )
    .bind(&key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);

    let report = drain_once(&pool, &Mailer::Log, 3).await.unwrap();
    assert_eq!(report.sent, 1);
    let (status, attempts, html, text): (String, i32, String, String) =
      sqlx::query_as(
        "SELECT status, attempts, html_body, text_body FROM email_outbox WHERE dedupe_key = $1",
      )
      .bind(&key)
      .fetch_one(&pool)
      .await
      .unwrap();
    assert_eq!(status, "sent");
    assert_eq!(attempts, 1);
    assert!(
      html.is_empty() && text.is_empty(),
      "bodies must be scrubbed"
    );
    // Nothing left to claim.
    let again = drain_once(&pool, &Mailer::Log, 3).await.unwrap();
    assert_eq!(again, DrainReport::default());
  }

  #[tokio::test]
  async fn auth_mail_is_capped_per_recipient() {
    let pool = crate::test_support::isolated_pool().await;
    let to = "Bombed@Example.com";
    let mut accepted = 0;
    for i in 0..8 {
      // Mixed kinds and address casing share one budget.
      let k = kind::RECIPIENT_THROTTLED[i % 3];
      let addr = if i % 2 == 0 {
        to.to_string()
      } else {
        to.to_lowercase()
      };
      if enqueue_throttled(&pool, k, &addr, &sample()).await.unwrap() {
        accepted += 1;
      }
    }
    assert_eq!(accepted, RECIPIENT_MAX_PER_WINDOW);
    let queued: i64 = sqlx::query_scalar(
      "SELECT count(*) FROM email_outbox WHERE lower(to_address) = 'bombed@example.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued, RECIPIENT_MAX_PER_WINDOW);

    // Other recipients and non-auth kinds are unaffected.
    assert!(
      enqueue_throttled(
        &pool,
        kind::PASSWORD_RESET,
        "other@example.com",
        &sample()
      )
      .await
      .unwrap()
    );
    enqueue(&pool, kind::ORDER_CREATED, to, &sample(), None)
      .await
      .unwrap();

    // Rows older than the window no longer count.
    sqlx::query(
      "UPDATE email_outbox SET created_at = now() - interval '2 hours'
       WHERE lower(to_address) = 'bombed@example.com'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
      enqueue_throttled(&pool, kind::VERIFY_EMAIL, to, &sample())
        .await
        .unwrap()
    );
  }

  #[tokio::test]
  async fn failing_send_backs_off_then_fails() {
    let pool = crate::test_support::isolated_pool().await;
    // Unparseable recipient: the SMTP mailer rejects it before any network
    // I/O, which exercises the retry path deterministically.
    sqlx::query(
      "INSERT INTO email_outbox (id, kind, to_address, subject, html_body, text_body)
       VALUES ($1, 'test', 'not-an-address', 's', 'h', 't')",
    )
    .bind(Uuid::now_v7())
    .execute(&pool)
    .await
    .unwrap();
    let mailer = Mailer::from_config(&crate::config::MailConfig {
      transport: crate::config::MailTransport::Smtp(
        crate::config::SmtpConfig {
          host: "127.0.0.1".to_string(),
          port: 1,
          tls: crate::config::SmtpTls::None,
          username: None,
          password: None,
        },
      ),
      from: "Lindaflor <loja@example.com>".to_string(),
    })
    .unwrap();

    let first = drain_once(&pool, &mailer, 2).await.unwrap();
    assert_eq!(first.retried, 1);
    let (status, delay_secs): (String, f64) = sqlx::query_as(
      "SELECT status, EXTRACT(EPOCH FROM next_attempt_at - now())::float8 FROM email_outbox",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    assert!(delay_secs > 20.0 && delay_secs <= 31.0, "{delay_secs}");

    // Make it due again; the second attempt hits the cap.
    sqlx::query("UPDATE email_outbox SET next_attempt_at = now()")
      .execute(&pool)
      .await
      .unwrap();
    let second = drain_once(&pool, &mailer, 2).await.unwrap();
    assert_eq!(second.failed, 1);
    let (status, html, last_error): (String, String, Option<String>) =
      sqlx::query_as("SELECT status, html_body, last_error FROM email_outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "failed");
    assert!(html.is_empty());
    assert!(last_error.is_some());
  }
}
