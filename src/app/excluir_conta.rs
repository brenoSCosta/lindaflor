//! Landing page for the delete-account confirmation link (see
//! `crate::mail::auth::DELETE_ACCOUNT_PATH`). GET only renders a button:
//! deletion happens on POST, so link scanners that prefetch URLs cannot
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

use crate::auth::routes::delete_user::complete_delete;
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

#[page(GET "/excluir-conta")]
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
                              "O link de exclusão pode estar expirado ou já foi usado. Solicite novamente nas configurações da conta."
                          )
                      )
                  } else {
                      card_header(
                          card_title("Excluir conta")
                          card_description("Esta ação é permanente e não pode ser desfeita.")
                      )
                      card_content(
                          <form method="post" action=(href!(confirm_post)) class="flex flex-col gap-4">
                              <input type="hidden" name="token" value=(token)>
                              <button type="submit" class=(button_variants(ButtonVariant::Destructive, ButtonSize::Md))>
                                  "Excluir minha conta"
                              </button>
                          </form>
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

#[route(POST "/excluir-conta")]
pub async fn confirm_post(
  cx: &Cx,
  Form(body): Form<ConfirmInput>,
) -> Result<Response> {
  let token = body.token.unwrap_or_default().trim().to_string();
  if token.is_empty() {
    return toast_redirect(cx, href!(page).resolve(cx));
  }
  match complete_delete(cx, &token).await {
    Ok(_) => {
      set_toast(cx, Toast::success("Conta excluída."));
      toast_redirect(cx, href!(crate::app::page).resolve(cx))
    }
    Err(err) => {
      set_toast(cx, Toast::error(portuguese_error_message(&err)));
      toast_redirect(cx, href!(page).resolve(cx))
    }
  }
}
