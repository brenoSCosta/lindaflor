use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    href, page, route,
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
use crate::components::input::input;
use crate::components::label::label;
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct ForgotInput {
  email: Option<String>,
}

#[page(GET "/forgot-password")]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      container(
          variant: ContainerVariant::Centered,
          <div class="w-full">
              <div class="mb-8 text-center">
                  <a href=(href!(crate::app::page)) class="text-2xl font-bold text-primary">"Linda Flor"</a>
              </div>
              card(
                  card_header(
                      card_title("Esqueceu a senha")
                      card_description(
                          "Informe seu e-mail e enviaremos um link para redefinir sua senha."
                      )
                  )
                  card_content(
                      <form method="post" action=(href!(forgot_post)) class="flex flex-col gap-4">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="email" }, "E-mail")
                              input(attrs: attributes! { type="email" name="email" id="email" required="" })
                          </div>
                          button(
                              variant: ButtonVariant::Primary,
                              attrs: attributes! { type="submit" },
                              "Enviar link"
                          )
                      </form>
                  )
                  card_footer(
                      <a href=(href!(crate::app::login::page)) class="text-sm text-primary">
                          "Voltar para o login"
                      </a>
                  )
              )
          </div>
      )
  })
}

#[page(GET "/forgot-password/sent")]
pub async fn sent() -> Result<impl View> {
  Ok(view! {
      container(
          variant: ContainerVariant::Centered,
          <div class="w-full">
              <div class="mb-8 text-center">
                  <a href=(href!(crate::app::page)) class="text-2xl font-bold text-primary">"Linda Flor"</a>
              </div>
              card(
                  card_header(
                      card_title("Link enviado")
                      card_description(
                          "Se existir uma conta com esse e-mail, um link de redefinição foi enviado. Verifique sua caixa de entrada."
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
              )
          </div>
      )
  })
}

#[route(POST "/forgot-password")]
pub async fn forgot_post(
  cx: &Cx,
  Form(body): Form<ForgotInput>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  let email = body.email.as_deref().unwrap_or("");
  let _ =
    service::request_password_reset(pool, email, Some("/reset-password")).await;
  Ok(see_other(href!(sent).resolve(cx)))
}
