use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    href, page, query_params, route,
  },
  view::{View, view},
};

use crate::auth::service;
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct ResendInput {
  email: Option<String>,
}

#[query_params(error = bad_request)]
struct CheckEmailQuery {
  email: Option<String>,
  resent: Option<String>,
}

#[page(GET "/check-email")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<CheckEmailQuery>(cx)?;
  let resent = query.resent.is_some();
  let email = query.email.clone().unwrap_or_default();

  Ok(view! {
      container(
          variant: ContainerVariant::Centered,
          <div class="w-full">
              <div class="mb-8 text-center">
                  <a href=(href!(crate::app::page)) class="text-2xl font-bold text-primary">"Linda Flor"</a>
              </div>
              card(
                  if resent {
                      card_header(
                          card_title("E-mail reenviado")
                          card_description(
                              "Enviamos um novo link de verificação. Verifique sua caixa de entrada."
                          )
                      )
                      card_footer(
                          <a
                              href=(href!(crate::app::login::page))
                              class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                          >
                              "Voltar para o login"
                          </a>
                      )
                  } else {
                      card_header(
                          card_title("Verifique sua caixa de entrada")
                          card_description(
                              if email.is_empty() {
                                  "Enviamos um link de verificação para o seu e-mail. Clique no link para concluir o login."
                              } else {
                                  <span>
                                      "Enviamos um link de verificação para "
                                      <strong>(email.clone())</strong>
                                      ". Clique no link para concluir o login."
                                  </span>
                              }
                          )
                      )
                      card_content(
                          <form method="post" action=(href!(resend))>
                              <input type="hidden" name="email" value=(email.clone())>
                              button(
                                  variant: ButtonVariant::Primary,
                                  attrs: attributes! { type="submit" },
                                  "Reenviar e-mail de verificação"
                              )
                          </form>
                      )
                      card_footer(
                          <a href=(href!(crate::app::login::page)) class="text-sm text-primary">
                              "Voltar para o login"
                          </a>
                      )
                  }
              )
          </div>
      )
  })
}

#[route(POST "/check-email/resend")]
pub async fn resend(
  cx: &Cx,
  Form(body): Form<ResendInput>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<CheckEmailQuery>(cx)?;
  let email = if let Some(email) = body
    .email
    .as_deref()
    .map(str::trim)
    .filter(|e| !e.is_empty())
  {
    email.to_owned()
  } else if let Some(email) = query
    .email
    .as_deref()
    .map(str::trim)
    .filter(|e| !e.is_empty())
  {
    email.to_owned()
  } else {
    return Ok(see_other(href!(page).resolve(cx)));
  };

  let callback = href!(crate::app::verify_email::page).resolve(cx);
  let _ =
    service::send_verification_email(pool, &email, Some(callback.as_str()))
      .await;
  Ok(see_other(
    href!(page)
      .query([("email", email.as_str()), ("resent", "1")])
      .resolve(cx),
  ))
}
