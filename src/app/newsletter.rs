use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    href, route,
  },
};

use crate::auth::validation::validate_email;
use crate::components::toast::{Toast, set_toast};
use crate::rate_limit::check_ip_rate;

/// Longest stored subscriber name; longer input is truncated.
const NAME_MAX_CHARS: usize = 100;

/// Form data for newsletter subscription.
#[derive(Deserialize)]
pub struct NewsletterForm {
  pub email: String,
  pub name: Option<String>,
}

/// Result of [`subscribe_email`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscribeOutcome {
  /// New subscriber; the welcome mail was queued in the same transaction.
  /// Carries the first-purchase coupon advertised in it, if any.
  Subscribed { first_purchase_code: Option<String> },
  /// The address (case-insensitive) was already on the list.
  AlreadySubscribed,
}

/// Insert the subscriber and queue the welcome mail atomically. Only a
/// real duplicate maps to [`SubscribeOutcome::AlreadySubscribed`]; every
/// other database error is returned to the caller.
pub async fn subscribe_email(
  pool: &PgPool,
  email: &str,
  name: Option<&str>,
) -> Result<SubscribeOutcome, sqlx::Error> {
  let mut tx = pool.begin().await?;
  let inserted = sqlx::query!(
    "INSERT INTO newsletter_subscribers (id, email, name, created_at) VALUES (gen_random_uuid(), $1, $2, now()) ON CONFLICT (lower(email)) DO NOTHING",
    email,
    name
  )
  .execute(&mut *tx)
  .await;
  let inserted = match inserted {
    Ok(done) => done.rows_affected() > 0,
    // Belt and braces: a unique violation not covered by the conflict
    // target is still "already subscribed", nothing else is.
    Err(sqlx::Error::Database(db)) if db.is_unique_violation() => false,
    Err(error) => return Err(error),
  };
  if !inserted {
    tx.rollback().await?;
    return Ok(SubscribeOutcome::AlreadySubscribed);
  }
  let first_purchase_code =
    crate::app::store::coupons::first_purchase_code(&mut *tx)
      .await?
      .map(|code| code.trim().to_string())
      .filter(|code| !code.is_empty());
  crate::mail::newsletter::send_welcome(
    &mut *tx,
    email,
    name.unwrap_or(""),
    first_purchase_code.as_deref(),
  )
  .await?;
  tx.commit().await?;
  Ok(SubscribeOutcome::Subscribed {
    first_purchase_code,
  })
}

/// Subscribe to the newsletter.
/// Rate limited to 5 requests per minute per IP address.
#[route(POST "/newsletter")]
pub async fn subscribe(
  cx: &Cx,
  Form(data): Form<NewsletterForm>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  let home = || see_other(href!(crate::app::page).resolve(cx));

  // Rate limit: 5 per minute per IP
  let ip = crate::valkey::client_ip(cx);

  if !check_ip_rate("newsletter", &ip, 5) {
    set_toast(
      cx,
      Toast::error("Muitas tentativas. Tente novamente em alguns minutos."),
    );
    return Ok(home());
  }

  if data.email.trim().is_empty() {
    set_toast(cx, Toast::error("E-mail é obrigatório"));
    return Ok(home());
  }
  let Ok(email) = validate_email(&data.email) else {
    set_toast(cx, Toast::error("E-mail inválido"));
    return Ok(home());
  };

  let name = data
    .name
    .as_deref()
    .map(str::trim)
    .filter(|n| !n.is_empty())
    .map(|n| n.chars().take(NAME_MAX_CHARS).collect::<String>());

  match subscribe_email(pool, &email, name.as_deref()).await {
    Ok(SubscribeOutcome::Subscribed {
      first_purchase_code,
    }) => {
      set_toast(
        cx,
        Toast::success(subscribed_message(first_purchase_code.as_deref())),
      );
    }
    Ok(SubscribeOutcome::AlreadySubscribed) => {
      set_toast(cx, Toast::error("E-mail já cadastrado na newsletter"));
    }
    Err(error) => {
      tracing::error!(error = %error, "newsletter subscribe failed");
      set_toast(
        cx,
        Toast::error("Não foi possível concluir a inscrição. Tente novamente."),
      );
    }
  }
  Ok(home())
}

/// Success toast after subscribing; advertises the first-purchase coupon
/// when the store has one configured (the welcome mail repeats it).
fn subscribed_message(first_purchase_code: Option<&str>) -> String {
  match first_purchase_code
    .map(str::trim)
    .filter(|code| !code.is_empty())
  {
    Some(code) => {
      format!("Inscrição realizada! Use o cupom {code} na sua primeira compra.")
    }
    None => {
      "Inscrição realizada! Você vai receber nossas novidades por e-mail."
        .to_string()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn subscribed_message_mentions_first_purchase_coupon() {
    assert_eq!(
      subscribed_message(Some(" BEMVINDA10 ")),
      "Inscrição realizada! Use o cupom BEMVINDA10 na sua primeira compra."
    );
    for code in [None, Some("")] {
      let message = subscribed_message(code);
      assert_eq!(
        message,
        "Inscrição realizada! Você vai receber nossas novidades por e-mail."
      );
      assert!(!message.contains("Verifique"));
    }
  }

  #[test]
  fn newsletter_uses_strict_email_validation() {
    // The old check (`contains('@') && contains('.')`) accepted these.
    for bad in ["a.b@", "@x.com", "a b@x.com", "a@b@c.com", "a@x..com"] {
      assert!(validate_email(bad).is_err(), "{bad}");
    }
    assert_eq!(
      validate_email(" Ana@Example.COM ").unwrap(),
      "ana@example.com"
    );
  }

  #[tokio::test]
  async fn subscribe_queues_one_welcome_and_flags_duplicates() {
    let pool = crate::test_support::isolated_pool().await;
    let email = "news@example.com";
    let first = subscribe_email(&pool, email, Some("Ana")).await.unwrap();
    assert!(matches!(first, SubscribeOutcome::Subscribed { .. }));
    // Case-insensitive duplicate: no second row, no second mail.
    let again = subscribe_email(&pool, "NEWS@example.com", None)
      .await
      .unwrap();
    assert_eq!(again, SubscribeOutcome::AlreadySubscribed);

    let mails: Vec<(String, String)> = sqlx::query_as(
      "SELECT kind, text_body FROM email_outbox WHERE lower(to_address) = $1",
    )
    .bind(email)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].0, "newsletter_welcome");
    assert!(mails[0].1.contains("Olá, Ana!"));
  }

  #[tokio::test]
  async fn subscribe_reports_real_db_errors() {
    let pool = crate::test_support::isolated_pool().await;
    sqlx::query("DROP TABLE newsletter_subscribers")
      .execute(&pool)
      .await
      .unwrap();
    let error = subscribe_email(&pool, "x@example.com", None).await;
    assert!(error.is_err(), "missing table must not read as a duplicate");
  }
}
