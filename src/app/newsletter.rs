use http::StatusCode;
use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    href, page, query_params,
    response::{IntoResponse, Response},
    route,
  },
  view::{View, view},
};

use crate::auth::validation::validate_email;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::toast::{Toast, set_toast, toast_redirect};
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
  /// The address had unsubscribed and is active again. The welcome mail is
  /// deduplicated per address, so it is not sent a second time.
  Resubscribed,
  /// The address (case-insensitive) was already on the list.
  AlreadySubscribed,
}

/// Insert the subscriber and queue the welcome mail atomically. An address
/// that had unsubscribed is reactivated. Only a real (active) duplicate
/// maps to [`SubscribeOutcome::AlreadySubscribed`]; every other database
/// error is returned to the caller.
pub async fn subscribe_email(
  pool: &PgPool,
  email: &str,
  name: Option<&str>,
) -> Result<SubscribeOutcome, sqlx::Error> {
  let mut tx = pool.begin().await?;
  // `xmax = 0` on the returned row: a fresh insert, not the conflict
  // update. No row at all: the address is already active.
  let upserted = sqlx::query_scalar!(
    r#"INSERT INTO newsletter_subscribers (id, email, name, created_at)
       VALUES (gen_random_uuid(), $1, $2, now())
       ON CONFLICT (lower(email)) DO UPDATE
         SET unsubscribed_at = NULL,
             name = COALESCE(EXCLUDED.name, newsletter_subscribers.name)
         WHERE newsletter_subscribers.unsubscribed_at IS NOT NULL
       RETURNING (xmax::text = '0') AS "inserted!""#,
    email,
    name
  )
  .fetch_optional(&mut *tx)
  .await;
  let inserted = match upserted {
    Ok(Some(true)) => true,
    Ok(Some(false)) => {
      tx.commit().await?;
      return Ok(SubscribeOutcome::Resubscribed);
    }
    Ok(None) => false,
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
    Ok(SubscribeOutcome::Resubscribed) => {
      set_toast(
        cx,
        Toast::success(
          "Inscrição reativada! Você voltará a receber nossas novidades.",
        ),
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

// --- Unsubscribe (LGPD) ---
//
// Every newsletter mail carries a signed link (plus `List-Unsubscribe` /
// `List-Unsubscribe-Post` headers, see `mail::outbox`). The token is an
// HMAC of the normalized address, so no DB token is needed and links never
// expire; rotating `TOKEN_PEPPER` invalidates outstanding links.

/// Path of the unsubscribe page (GET confirm button, POST unsubscribes).
pub const UNSUBSCRIBE_PATH: &str = "/newsletter/cancelar";

/// Dev/test key when `TOKEN_PEPPER` is unset (production sets it).
const DEV_UNSUBSCRIBE_KEY: &str = "lindaflor-dev-newsletter-unsubscribe";

/// Domain tag of the newsletter unsubscribe token.
const UNSUBSCRIBE_DOMAIN: &str = "newsletter-unsubscribe:v1:";

/// HMAC-SHA256 over `domain` + the normalized address, keyed by the token
/// pepper. `domain` separates each link kind (and the verification-token
/// hashes on the same key), so a token minted for one purpose never
/// verifies for another.
fn email_mac(
  domain: &str,
  email: &str,
  pepper: &str,
) -> hmac::Hmac<sha2::Sha256> {
  use hmac::Mac;
  let key = if pepper.is_empty() {
    DEV_UNSUBSCRIBE_KEY
  } else {
    pepper
  };
  let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key.as_bytes())
    .expect("HMAC accepts any key length");
  mac.update(domain.as_bytes());
  mac.update(email.trim().to_lowercase().as_bytes());
  mac
}

/// Signed, non-expiring token binding `email` (case-insensitive) to the
/// purpose `domain` (e.g. `"stock-alert-cancel:v1:"`), hex HMAC-SHA256.
/// Shared by every one-click mail link; rotating `TOKEN_PEPPER`
/// invalidates outstanding links.
pub fn signed_email_token(domain: &str, email: &str) -> String {
  signed_email_token_with_pepper(domain, email, &crate::config::token_pepper())
}

pub fn signed_email_token_with_pepper(
  domain: &str,
  email: &str,
  pepper: &str,
) -> String {
  use hmac::Mac;
  hex::encode(email_mac(domain, email, pepper).finalize().into_bytes())
}

/// Constant-time check of a [`signed_email_token`].
pub fn verify_signed_email_token(
  domain: &str,
  email: &str,
  token: &str,
) -> bool {
  verify_signed_email_token_with_pepper(
    domain,
    email,
    token,
    &crate::config::token_pepper(),
  )
}

pub fn verify_signed_email_token_with_pepper(
  domain: &str,
  email: &str,
  token: &str,
  pepper: &str,
) -> bool {
  use hmac::Mac;
  let Ok(raw) = hex::decode(token.trim()) else {
    return false;
  };
  email_mac(domain, email, pepper).verify_slice(&raw).is_ok()
}

/// Token for `email` (case-insensitive), hex HMAC-SHA256.
pub fn unsubscribe_token_with_pepper(email: &str, pepper: &str) -> String {
  signed_email_token_with_pepper(UNSUBSCRIBE_DOMAIN, email, pepper)
}

/// Constant-time check of an unsubscribe token.
pub fn verify_unsubscribe_token_with_pepper(
  email: &str,
  token: &str,
  pepper: &str,
) -> bool {
  verify_signed_email_token_with_pepper(
    UNSUBSCRIBE_DOMAIN,
    email,
    token,
    pepper,
  )
}

fn verify_unsubscribe_token(email: &str, token: &str) -> bool {
  verify_unsubscribe_token_with_pepper(
    email,
    token,
    &crate::config::token_pepper(),
  )
}

/// Absolute one-click unsubscribe link for `email`.
pub fn unsubscribe_url(email: &str) -> String {
  let email = email.trim().to_lowercase();
  let token =
    unsubscribe_token_with_pepper(&email, &crate::config::token_pepper());
  crate::mail::absolute_url(&format!(
    "{UNSUBSCRIBE_PATH}?email={}&token={token}",
    crate::mail::encode_query_value(&email)
  ))
}

/// Flag the address as unsubscribed (idempotent). Returns whether an
/// active subscription was cancelled. An address that was never on the list
/// gets a suppression row (already unsubscribed): the same link also stops
/// other marketing mail, e.g. abandoned-cart reminders
/// (`store::abandoned_cart`), so it must stick for non-subscribers too.
pub async fn unsubscribe_email(
  pool: &PgPool,
  email: &str,
) -> Result<bool, sqlx::Error> {
  // `xmax <> 0`: the conflict update (an active row was cancelled); a fresh
  // suppression insert reports false; no row: already unsubscribed.
  let cancelled = sqlx::query_scalar!(
    r#"INSERT INTO newsletter_subscribers (id, email, created_at, unsubscribed_at)
       VALUES (gen_random_uuid(), $1, now(), now())
       ON CONFLICT (lower(email)) DO UPDATE
         SET unsubscribed_at = now()
         WHERE newsletter_subscribers.unsubscribed_at IS NULL
       RETURNING (xmax::text <> '0') AS "cancelled!""#,
    email.trim().to_lowercase()
  )
  .fetch_optional(pool)
  .await?;
  Ok(cancelled.unwrap_or(false))
}

#[query_params(error = bad_request)]
struct UnsubscribeQuery {
  email: Option<String>,
  token: Option<String>,
  /// Set after a successful POST (`?feito=1`).
  feito: Option<String>,
}

/// Confirm form fields; a RFC 8058 one-click POST sends only
/// `List-Unsubscribe=One-Click` and keeps email/token in the query string.
#[derive(Deserialize)]
pub struct UnsubscribeForm {
  email: Option<String>,
  token: Option<String>,
  #[serde(rename = "List-Unsubscribe")]
  list_unsubscribe: Option<String>,
}

/// Landing page of the unsubscribe link. GET only renders a confirm
/// button, so link scanners that prefetch URLs never unsubscribe anyone.
#[page(GET "/newsletter/cancelar")]
pub async fn unsubscribe_page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<UnsubscribeQuery>(cx)?;
  let email = query.email.clone().unwrap_or_default();
  let token = query.token.clone().unwrap_or_default();
  let done = query.feito.is_some();
  let valid = !done && verify_unsubscribe_token(&email, &token);

  Ok(view! {
      container(
          variant: ContainerVariant::Centered,
          <div class="w-full">
              <div class="mb-8 text-center">
                  <a href=(href!(crate::app::page)) class="text-2xl font-bold text-primary">"Linda Flor"</a>
              </div>
              card(
                  if done {
                      card_header(
                          card_title("Inscrição cancelada")
                          card_description(
                              "Você não vai mais receber a newsletter. Se mudar de ideia, é só se inscrever de novo no rodapé da loja."
                          )
                      )
                  } else if valid {
                      card_header(
                          card_title("Cancelar newsletter")
                          card_description(
                              "Confirme para parar de receber nossos e-mails de novidades e promoções."
                          )
                      )
                      card_content(
                          <p class="mb-4 text-sm text-muted-foreground">(email.clone())</p>
                          <form method="post" action=(href!(unsubscribe_post)) class="flex flex-col gap-4">
                              <input type="hidden" name="email" value=(email.clone())>
                              <input type="hidden" name="token" value=(token.clone())>
                              <button type="submit" class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))>
                                  "Cancelar inscrição"
                              </button>
                          </form>
                      )
                  } else {
                      card_header(
                          card_title("Link inválido")
                          card_description(
                              "Este link de cancelamento não é válido. Use o link do e-mail mais recente da newsletter."
                          )
                      )
                  }
                  card_footer(
                      <a href=(href!(crate::app::page)) class="text-sm text-primary">
                          "Voltar para a loja"
                      </a>
                  )
              )
          </div>
      )
  })
}

/// Unsubscribe: the confirm form above, or a mail client's RFC 8058
/// one-click POST (email/token in the query string). Exempt from the
/// origin check in `src/app.rs` (mail providers post cross-origin); the
/// HMAC token is the authorization.
#[route(POST "/newsletter/cancelar")]
pub async fn unsubscribe_post(
  cx: &Cx,
  Form(form): Form<UnsubscribeForm>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<UnsubscribeQuery>(cx)?;
  let one_click = form.list_unsubscribe.is_some();
  let email = form
    .email
    .or_else(|| query.email.clone())
    .unwrap_or_default();
  let token = form
    .token
    .or_else(|| query.token.clone())
    .unwrap_or_default();

  if !verify_unsubscribe_token(&email, &token) {
    if one_click {
      return (StatusCode::BAD_REQUEST, "invalid token").into_response(cx);
    }
    set_toast(cx, Toast::error("Link de cancelamento inválido."));
    return toast_redirect(cx, href!(unsubscribe_page).resolve(cx));
  }
  if let Err(error) = unsubscribe_email(pool, &email).await {
    tracing::error!(error = %error, "newsletter unsubscribe failed");
    if one_click {
      return (StatusCode::SERVICE_UNAVAILABLE, "retry").into_response(cx);
    }
    set_toast(
      cx,
      Toast::error("Não foi possível cancelar a inscrição. Tente novamente."),
    );
    return toast_redirect(cx, href!(crate::app::page).resolve(cx));
  }
  if one_click {
    return (StatusCode::OK, "unsubscribed").into_response(cx);
  }
  toast_redirect(
    cx,
    href!(unsubscribe_page).query([("feito", "1")]).resolve(cx),
  )
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

  #[test]
  fn unsubscribe_token_is_signed_per_address() {
    let token = unsubscribe_token_with_pepper("Ana@Example.com", "pepper");
    assert_eq!(token.len(), 64);
    // Case-insensitive, whitespace-tolerant.
    assert!(verify_unsubscribe_token_with_pepper(
      " ana@example.com ",
      &token,
      "pepper"
    ));
    assert!(!verify_unsubscribe_token_with_pepper(
      "bia@example.com",
      &token,
      "pepper"
    ));
    assert!(!verify_unsubscribe_token_with_pepper(
      "ana@example.com",
      &token,
      "other-pepper"
    ));
    for bad in ["", "zz", &token[..62]] {
      assert!(!verify_unsubscribe_token_with_pepper(
        "ana@example.com",
        bad,
        "pepper"
      ));
    }
    // No pepper (dev/test): still keyed, never a bare hash of the address.
    let dev = unsubscribe_token_with_pepper("ana@example.com", "");
    assert!(verify_unsubscribe_token_with_pepper(
      "ana@example.com",
      &dev,
      ""
    ));
    assert_ne!(dev, token);
  }

  #[test]
  fn unsubscribe_url_carries_email_and_valid_token() {
    let url = unsubscribe_url("Ana+news@Example.com");
    assert!(url.contains(UNSUBSCRIBE_PATH));
    assert!(url.contains("email=ana%2Bnews%40example.com&token="));
    let token = url.rsplit_once("token=").unwrap().1;
    assert!(verify_unsubscribe_token("ana+news@example.com", token));
  }

  #[tokio::test]
  async fn unsubscribe_then_resubscribe() {
    let pool = crate::test_support::isolated_pool().await;
    let email = "leave@example.com";
    assert!(matches!(
      subscribe_email(&pool, email, Some("Ana")).await.unwrap(),
      SubscribeOutcome::Subscribed { .. }
    ));
    assert!(unsubscribe_email(&pool, "LEAVE@example.com").await.unwrap());
    // Idempotent; unknown addresses are a silent no-op.
    assert!(!unsubscribe_email(&pool, email).await.unwrap());
    assert!(
      !unsubscribe_email(&pool, "nobody@example.com")
        .await
        .unwrap()
    );
    let active: bool = sqlx::query_scalar(
      "SELECT unsubscribed_at IS NULL FROM newsletter_subscribers WHERE email = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!active);

    // Re-subscribing reactivates the row without a second welcome mail.
    assert_eq!(
      subscribe_email(&pool, email, None).await.unwrap(),
      SubscribeOutcome::Resubscribed
    );
    let (active, name): (bool, Option<String>) = sqlx::query_as(
      "SELECT unsubscribed_at IS NULL, name FROM newsletter_subscribers WHERE email = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(active);
    assert_eq!(name.as_deref(), Some("Ana"));
    assert_eq!(
      subscribe_email(&pool, email, None).await.unwrap(),
      SubscribeOutcome::AlreadySubscribed
    );
    let mails: i64 = sqlx::query_scalar(
      "SELECT count(*) FROM email_outbox WHERE to_address = $1",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mails, 1);
  }

  #[tokio::test]
  async fn welcome_mail_links_to_unsubscribe() {
    let pool = crate::test_support::isolated_pool().await;
    subscribe_email(&pool, "link@example.com", None)
      .await
      .unwrap();
    let text: String = sqlx::query_scalar(
      "SELECT text_body FROM email_outbox WHERE to_address = 'link@example.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
      text.contains(&format!(
        "Cancelar inscrição: {}",
        unsubscribe_url("link@example.com")
      )),
      "{text}"
    );
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
