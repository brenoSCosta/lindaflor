//! Newsletter welcome mail, queued with the subscription row (see
//! `app::newsletter`). Deduplicated per address: re-subscribing (or a
//! request for someone else's address) never sends a second copy.

use sqlx::PgExecutor;

use super::outbox::{enqueue, kind};
use super::{absolute_url, templates};

pub async fn send_welcome<'e>(
  executor: impl PgExecutor<'e>,
  to: &str,
  name: &str,
  first_purchase_code: Option<&str>,
) -> Result<(), sqlx::Error> {
  let email = templates::newsletter_welcome(
    name,
    &absolute_url("/"),
    first_purchase_code,
  );
  let dedupe = format!("newsletter-welcome:{}", to.trim().to_lowercase());
  enqueue(
    executor,
    kind::NEWSLETTER_WELCOME,
    to,
    &email,
    Some(&dedupe),
  )
  .await
}
