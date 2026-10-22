//! "Avise-me quando chegar": the back-in-stock form shown on the product
//! page for a sold-out variant, its subscribe endpoint, and the signed
//! cancel page linked from the alert mail. Storage and delivery live in
//! [`crate::app::store::stock_alerts`].

use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    href, page, query_params,
    response::Response,
    route,
  },
  view::{View, attributes, component, view},
};
use uuid::Uuid;

use crate::app::store::stock_alerts::{
  self, SubscribeOutcome, verify_cancel_token,
};
use crate::auth::user::current_user_owned;
use crate::auth::validation::validate_email;
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::toast::{Toast, set_toast, toast_redirect};
use crate::rate_limit::check_ip_rate;

/// Subscribe attempts per IP per minute.
const SUBSCRIBE_PER_MIN: u32 = 5;

/// Bot trap: a visually hidden field humans leave empty.
const HONEYPOT_FIELD: &str = "website";

#[derive(Deserialize)]
pub struct StockAlertForm {
  variant_id: String,
  email: String,
  #[serde(default)]
  website: Option<String>,
}

/// Back-in-stock form for a sold-out variant. Signed-in shoppers get their
/// account email prefilled.
#[component]
pub async fn stock_alert_form(
  cx: &Cx,
  variant_id: String,
) -> Result<impl View> {
  let email = current_user_owned(cx)
    .await?
    .map(|session| session.user.email)
    .unwrap_or_default();
  Ok(view! {
      card(
          card_header(
              card_title("Avise-me quando chegar")
              card_description(
                  "Esta variação está esgotada. Deixe seu e-mail e avisamos uma única vez quando ela voltar ao estoque."
              )
          )
          card_content(
              <form method="post" action=(href!(subscribe)) class="space-y-3">
                  <input type="hidden" name="variant_id" value=(variant_id)>
                  <div class="hidden" aria-hidden="true">
                      <input type="text" name=(HONEYPOT_FIELD) tabindex="-1" autocomplete="off">
                  </div>
                  <div class="space-y-2">
                      label(attrs: attributes! { for="stock-alert-email" }, "E-mail")
                      input(attrs: attributes! {
                          type="email"
                          name="email"
                          id="stock-alert-email"
                          autocomplete="email"
                          placeholder="seu@email.com"
                          required=""
                          value=(email)
                      })
                  </div>
                  button(
                      variant: ButtonVariant::Outline,
                      size: ButtonSize::Lg,
                      attrs: attributes! { type="submit" class="w-full" },
                      "Avise-me quando chegar"
                  )
              </form>
          )
      )
  })
}

/// Register a back-in-stock alert, then return to the product page with a
/// toast. Rate limited per IP; a filled honeypot is answered as success.
#[route(POST "/avise-me")]
pub async fn subscribe(
  cx: &Cx,
  Form(form): Form<StockAlertForm>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  let variant_id = Uuid::parse_str(form.variant_id.trim()).ok();
  let slug = match variant_id {
    Some(id) => stock_alerts::variant_slug(pool, id).await?,
    None => None,
  };
  // Plain paths (not `href!`): this route is also mounted without the
  // file-routed storefront pages (integration-test router).
  let (Some(variant_id), Some(slug)) = (variant_id, slug) else {
    set_toast(cx, Toast::error("Esta variação não está disponível."));
    return Ok(see_other("/produtos"));
  };
  let back = see_other(format!(
    "/produtos/{}?variant={variant_id}",
    crate::mail::encode_query_value(&slug)
  ));

  if !check_ip_rate(
    "stock-alert",
    &crate::valkey::client_ip(cx),
    SUBSCRIBE_PER_MIN,
  ) {
    set_toast(
      cx,
      Toast::error("Muitas tentativas. Tente novamente em alguns minutos."),
    );
    return Ok(back);
  }
  if form
    .website
    .as_deref()
    .is_some_and(|v| !v.trim().is_empty())
  {
    tracing::info!("stock alert honeypot tripped; dropped");
    set_toast(cx, Toast::success(SUBSCRIBED_MESSAGE));
    return Ok(back);
  }
  let Ok(email) = validate_email(&form.email) else {
    set_toast(cx, Toast::error("Informe um e-mail válido."));
    return Ok(back);
  };
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);

  let toast = match stock_alerts::subscribe(pool, variant_id, &email, user_id)
    .await
  {
    Ok(SubscribeOutcome::Subscribed | SubscribeOutcome::AlreadySubscribed) => {
      Toast::success(SUBSCRIBED_MESSAGE)
    }
    Ok(SubscribeOutcome::InStock) => {
      Toast::success("Boa notícia: esta variação já está disponível!")
    }
    Ok(SubscribeOutcome::TooMany) => Toast::error(
      "Este e-mail já tem muitos avisos pendentes. Cancele algum pelo link do último aviso e tente de novo.",
    ),
    Ok(SubscribeOutcome::UnknownVariant) => {
      Toast::error("Esta variação não está disponível.")
    }
    Err(error) => {
      tracing::error!(error = %error, "stock alert subscribe failed");
      Toast::error("Não foi possível registrar o aviso. Tente novamente.")
    }
  };
  set_toast(cx, toast);
  Ok(back)
}

/// Same answer for new and repeated subscriptions (no address probing).
const SUBSCRIBED_MESSAGE: &str =
  "Pronto! Avisaremos por e-mail quando esta peça voltar ao estoque.";

#[query_params(error = bad_request)]
struct CancelQuery {
  email: Option<String>,
  token: Option<String>,
  /// Set after a successful POST (`?feito=1`).
  feito: Option<String>,
}

#[derive(Deserialize)]
pub struct CancelForm {
  email: Option<String>,
  token: Option<String>,
}

/// Landing page of the cancel link. GET only renders a confirm button, so
/// link scanners that prefetch URLs never cancel anything.
#[page(GET "/avise-me/cancelar")]
pub async fn cancel_page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<CancelQuery>(cx)?;
  let email = query.email.clone().unwrap_or_default();
  let token = query.token.clone().unwrap_or_default();
  let done = query.feito.is_some();
  let valid = !done && verify_cancel_token(&email, &token);

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
                          card_title("Avisos cancelados")
                          card_description(
                              "Você não vai mais receber avisos de volta ao estoque para este e-mail."
                          )
                      )
                  } else if valid {
                      card_header(
                          card_title("Cancelar avisos de estoque")
                          card_description(
                              "Confirme para cancelar todos os avisos \"Avise-me quando chegar\" pendentes deste e-mail."
                          )
                      )
                      card_content(
                          <p class="mb-4 text-sm text-muted-foreground">(email.clone())</p>
                          <form method="post" action=(href!(cancel_post)) class="flex flex-col gap-4">
                              <input type="hidden" name="email" value=(email.clone())>
                              <input type="hidden" name="token" value=(token.clone())>
                              <button type="submit" class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))>
                                  "Cancelar avisos"
                              </button>
                          </form>
                      )
                  } else {
                      card_header(
                          card_title("Link inválido")
                          card_description(
                              "Este link de cancelamento não é válido. Use o link do e-mail de aviso mais recente."
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

/// Cancel every pending alert of the address; the HMAC token is the
/// authorization.
#[route(POST "/avise-me/cancelar")]
pub async fn cancel_post(
  cx: &Cx,
  Form(form): Form<CancelForm>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let email = form.email.unwrap_or_default();
  let token = form.token.unwrap_or_default();
  if !verify_cancel_token(&email, &token) {
    set_toast(cx, Toast::error("Link de cancelamento inválido."));
    return toast_redirect(cx, href!(cancel_page).resolve(cx));
  }
  if let Err(error) = stock_alerts::cancel_all(pool, &email).await {
    tracing::error!(error = %error, "stock alert cancel failed");
    set_toast(
      cx,
      Toast::error("Não foi possível cancelar os avisos. Tente novamente."),
    );
    return toast_redirect(cx, href!(crate::app::page).resolve(cx));
  }
  toast_redirect(cx, href!(cancel_page).query([("feito", "1")]).resolve(cx))
}

#[cfg(test)]
mod tests {
  #[test]
  fn cancel_path_matches_route() {
    assert_eq!(
      crate::app::store::stock_alerts::CANCEL_PATH,
      "/avise-me/cancelar"
    );
  }
}
