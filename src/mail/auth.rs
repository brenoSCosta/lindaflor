//! Auth mail hooks: build the absolute link for a freshly issued one-time
//! token and enqueue the message. The raw token only ever lives in the
//! outbox body (scrubbed once delivered) — never in logs.

use sqlx::PgExecutor;

use super::outbox::{enqueue, kind};
use super::{absolute_url, encode_query_value, templates};

fn token_url(path: &str, token: &str) -> String {
  absolute_url(&format!("{path}?token={}", encode_query_value(token)))
}

/// Landing pages for each link (see `src/app/*.rs`).
pub const VERIFY_EMAIL_PATH: &str = "/verify-email";
pub const RESET_PASSWORD_PATH: &str = "/reset-password";
pub const CHANGE_EMAIL_PATH: &str = "/confirmar-email";
pub const DELETE_ACCOUNT_PATH: &str = "/excluir-conta";

pub async fn send_verify_email<'e>(
  executor: impl PgExecutor<'e>,
  to: &str,
  name: &str,
  token: &str,
) -> Result<(), sqlx::Error> {
  let email =
    templates::verify_email(name, &token_url(VERIFY_EMAIL_PATH, token));
  enqueue(executor, kind::VERIFY_EMAIL, to, &email, None).await
}

pub async fn send_password_reset<'e>(
  executor: impl PgExecutor<'e>,
  to: &str,
  name: &str,
  token: &str,
) -> Result<(), sqlx::Error> {
  let email =
    templates::password_reset(name, &token_url(RESET_PASSWORD_PATH, token));
  enqueue(executor, kind::PASSWORD_RESET, to, &email, None).await
}

/// Sent to the *new* address: following the link proves ownership of it.
pub async fn send_change_email<'e>(
  executor: impl PgExecutor<'e>,
  new_email: &str,
  name: &str,
  token: &str,
) -> Result<(), sqlx::Error> {
  let email = templates::change_email(
    name,
    new_email,
    &token_url(CHANGE_EMAIL_PATH, token),
  );
  enqueue(executor, kind::CHANGE_EMAIL, new_email, &email, None).await
}

pub async fn send_delete_account<'e>(
  executor: impl PgExecutor<'e>,
  to: &str,
  name: &str,
  token: &str,
) -> Result<(), sqlx::Error> {
  let email =
    templates::delete_account(name, &token_url(DELETE_ACCOUNT_PATH, token));
  enqueue(executor, kind::DELETE_ACCOUNT, to, &email, None).await
}
