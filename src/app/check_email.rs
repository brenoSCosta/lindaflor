use serde::Deserialize;
use topcoat::{
  Result,
  context::Cx,
  router::{content::Form, page, query_params},
  view::{View, view},
};

use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct ResendInput {
  #[allow(dead_code)]
  email: Option<String>,
}

#[query_params(error = bad_request)]
struct CheckEmailQuery {
  email: Option<String>,
}

#[page([GET, POST] "/check-email")]
pub async fn page(
  cx: &Cx,
  body: Option<Form<ResendInput>>,
) -> Result<impl View> {
  let query = query_params::<CheckEmailQuery>(cx)?;
  let resent =
    body.is_some() && topcoat::router::request::method(cx).as_str() == "POST";
  let email = query.email.clone().unwrap_or_default();

  Ok(view! {
      <div class="flex min-h-screen items-center justify-center bg-background px-4 py-8">
          <div class="w-full max-w-md">
              <div class="mb-8 text-center">
                  <a href="/" class="text-2xl font-bold text-primary">"Linda Flor"</a>
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
                              href="/login"
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
                                      <strong>(email)</strong>
                                      ". Clique no link para concluir o login."
                                  </span>
                              }
                          )
                      )
                      card_content(
                          <form method="post" action="/check-email">
                              button(
                                  variant: ButtonVariant::Primary,
                                  attrs: attributes! { type="submit" },
                                  "Reenviar e-mail de verificação"
                              )
                          </form>
                      )
                      card_footer(
                          <a href="/login" class="text-sm text-primary">
                              "Voltar para o login"
                          </a>
                      )
                  }
              )
          </div>
      </div>
  })
}
