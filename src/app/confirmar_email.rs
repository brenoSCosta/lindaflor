//! Landing page for the change-email confirmation link (see
//! `crate::mail::auth::CHANGE_EMAIL_PATH`). GET only renders a button: the
//! change happens on POST, so link scanners that prefetch URLs cannot
//! consume the token.

use serde::Deserialize;
use topcoat::{
  Result,
  context::Cx,
  router::{
    content::Form, href, page, query_params, response::Response, route,
  },
  view::{View, view},
};

use crate::auth::routes::change_email::complete_change_email;
use crate::auth::service::portuguese_error_message;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::toast::{Toast, set_toast, toast_redirect};

#[derive(Deserialize)]
pub struct ConfirmInput {
  token: Option<String>,
}

#[query_params(error = bad_request)]
struct ConfirmQuery {
  token: Option<String>,
}

#[page(GET "/confirmar-email")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<ConfirmQuery>(cx)?;
  let token = query.token.clone().unwrap_or_default();
  let has_token = !token.is_empty();

  Ok(view! {
      container(
          variant: ContainerVariant::Centered,
          <div class="w-full">
              <div class="mb-8 text-center">
                  <a href=(href!(crate::app::page)) class="text-2xl font-bold text-primary">"Linda Flor"</a>
              </div>
              card(
                  if !has_token {
                      card_header(
                          card_title("Link inválido")
                          card_description(
                              "O link de confirmação pode estar expirado ou já foi usado. Solicite a troca novamente nas configurações da conta."
                          )
                      )
                  } else {
                      card_header(
                          card_title("Confirmar novo e-mail")
                          card_description("Confirme para usar este endereço na sua conta. Você precisará entrar novamente.")
                      )
                      card_content(
                          <form method="post" action=(href!(confirm_post)) class="flex flex-col gap-4">
                              <input type="hidden" name="token" value=(token)>
                              <button type="submit" class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))>
                                  "Confirmar e-mail"
                              </button>
                          </form>
                      )
                  }
                  card_footer(
                      <a href=(href!(crate::app::login::page)) class="text-sm text-primary">
                          "Ir para o login"
                      </a>
                  )
              )
          </div>
      )
  })
}

#[route(POST "/confirmar-email")]
pub async fn confirm_post(
  cx: &Cx,
  Form(body): Form<ConfirmInput>,
) -> Result<Response> {
  let token = body.token.unwrap_or_default().trim().to_string();
  if token.is_empty() {
    return toast_redirect(cx, href!(page).resolve(cx));
  }
  match complete_change_email(cx, &token).await {
    Ok(_) => {
      set_toast(cx, Toast::success("E-mail alterado. Entre novamente."));
      toast_redirect(
        cx,
        href!(crate::app::login::page)
          .query([("mode", "signin")])
          .resolve(cx),
      )
    }
    Err(err) => {
      set_toast(cx, Toast::error(portuguese_error_message(&err)));
      toast_redirect(cx, href!(page).resolve(cx))
    }
  }
}
