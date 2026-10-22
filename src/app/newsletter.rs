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

use crate::components::toast::{Toast, set_toast};
use crate::rate_limit::check_ip_rate;

/// Form data for newsletter subscription.
#[derive(Deserialize)]
pub struct NewsletterForm {
  pub email: String,
  pub name: Option<String>,
}

/// Subscribe to the newsletter.
/// Rate limited to 5 requests per minute per IP address.
#[route(POST "/newsletter")]
pub async fn subscribe(
  cx: &Cx,
  Form(data): Form<NewsletterForm>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);

  // Rate limit: 5 per minute per IP
  let ip = crate::valkey::client_ip(cx);

  if !check_ip_rate("newsletter", &ip, 5) {
    set_toast(
      cx,
      Toast::error("Muitas tentativas. Tente novamente em alguns minutos."),
    );
    return Ok(see_other(href!(crate::app::page).resolve(cx)));
  }

  let email = data.email.trim().to_lowercase();
  if email.is_empty() {
    set_toast(cx, Toast::error("E-mail é obrigatório"));
    return Ok(see_other(href!(crate::app::page).resolve(cx)));
  }

  // Simple email validation
  if !email.contains('@') || !email.contains('.') {
    set_toast(cx, Toast::error("E-mail inválido"));
    return Ok(see_other(href!(crate::app::page).resolve(cx)));
  }

  let name = data
    .name
    .as_ref()
    .map(|n| n.trim())
    .filter(|n| !n.is_empty());

  match sqlx::query!(
    "INSERT INTO newsletter_subscribers (id, email, name, created_at) VALUES (gen_random_uuid(), $1, $2, now()) ON CONFLICT (lower(email)) DO NOTHING",
    email,
    name
  )
    .execute(pool)
    .await
  {
    Ok(_) => {
      let first_purchase_code =
        crate::app::store::coupons::first_purchase_code(pool)
          .await
          .ok()
          .flatten();
      set_toast(
        cx,
        Toast::success(subscribed_message(first_purchase_code.as_deref())),
      );
      Ok(see_other(href!(crate::app::page).resolve(cx)))
    }
    Err(_) => {
      set_toast(cx, Toast::error("E-mail já cadastrado na newsletter"));
      Ok(see_other(href!(crate::app::page).resolve(cx)))
    }
  }
}

/// Success toast after subscribing; advertises the first-purchase coupon
/// when the store has one configured.
fn subscribed_message(first_purchase_code: Option<&str>) -> String {
  match first_purchase_code
    .map(str::trim)
    .filter(|code| !code.is_empty())
  {
    Some(code) => {
      format!("Inscrição realizada! Use o cupom {code} na sua primeira compra.")
    }
    None => "Inscrição realizada! Verifique seu e-mail.".to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::subscribed_message;

  #[test]
  fn subscribed_message_mentions_first_purchase_coupon() {
    assert_eq!(
      subscribed_message(Some(" BEMVINDA10 ")),
      "Inscrição realizada! Use o cupom BEMVINDA10 na sua primeira compra."
    );
    for code in [None, Some("")] {
      assert_eq!(
        subscribed_message(code),
        "Inscrição realizada! Verifique seu e-mail."
      );
    }
  }
}
