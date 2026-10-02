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
pub struct InvitationInput {
  action: Option<String>,
}

#[query_params(error = bad_request)]
struct InvitationQuery {
  id: Option<String>,
  email: Option<String>,
}

fn role_label(role: &str) -> &'static str {
  match role {
    "owner" => "Proprietário",
    "admin" => "Administrador",
    _ => "Membro",
  }
}

#[page([GET, POST] "/accept-invitation")]
pub async fn page(
  cx: &Cx,
  body: Option<Form<InvitationInput>>,
) -> Result<impl View> {
  let query = query_params::<InvitationQuery>(cx)?;
  let invitation_id = query.id.clone().unwrap_or_default();
  let email = query.email.clone().unwrap_or_default();
  let accepted = body
    .as_ref()
    .map(|b| b.action.as_deref() == Some("accept"))
    .unwrap_or(false)
    && topcoat::router::request::method(cx).as_str() == "POST";
  let declined = body
    .as_ref()
    .map(|b| b.action.as_deref() == Some("decline"))
    .unwrap_or(false)
    && topcoat::router::request::method(cx).as_str() == "POST";

  Ok(view! {
      <div class="flex min-h-screen items-center justify-center bg-background px-4 py-8">
          <div class="w-full max-w-md">
              <div class="mb-8 text-center">
                  <a href="/" class="text-2xl font-bold text-primary">"Linda Flor"</a>
              </div>
              card(
                  if accepted {
                      card_header(
                          card_title("Convite aceito")
                          card_description("Você entrou na organização.")
                      )
                      card_footer(
                          <a
                              href="/permissions"
                              class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                          >
                              "Ver permissões"
                          </a>
                      )
                  } else if declined {
                      card_header(
                          card_title("Convite recusado")
                          card_description("Você recusou o convite para esta organização.")
                      )
                      card_footer(
                          <a
                              href="/"
                              class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                          >
                              "Voltar para a loja"
                          </a>
                      )
                  } else {
                      card_header(
                          card_title("Você foi convidado")
                          card_description(
                              if email.is_empty() {
                                  <span>
                                      "Você foi convidado para entrar na organização "
                                      <strong>"Linda Flor"</strong>
                                      " como "
                                      <strong>(role_label("member"))</strong>
                                      "."
                                  </span>
                              } else {
                                  <span>
                                      <strong>(email)</strong>
                                      " convidou você para entrar na organização "
                                      <strong>"Linda Flor"</strong>
                                      " como "
                                      <strong>(role_label("member"))</strong>
                                      "."
                                  </span>
                              }
                          )
                      )
                      card_content(
                          <p class="text-center text-sm text-muted-foreground">
                              "Aceitar adicionará você à organização e a tornará sua organização ativa."
                          </p>
                          <div class="mt-6 flex flex-col gap-3">
                              <form method="post" action=(format!("/accept-invitation?id={}", invitation_id))>
                                  <input type="hidden" name="action" value="accept" />
                                  button(
                                      variant: ButtonVariant::Primary,
                                      attrs: attributes! { type="submit" },
                                      "Aceitar convite"
                                  )
                              </form>
                              <form method="post" action=(format!("/accept-invitation?id={}", invitation_id))>
                                  <input type="hidden" name="action" value="decline" />
                                  button(
                                      variant: ButtonVariant::Outline,
                                      attrs: attributes! { type="submit" },
                                      "Recusar"
                                  )
                              </form>
                          </div>
                      )
                      card_footer(
                          <a href="/login" class="text-sm text-primary">
                              "Entrar com outra conta"
                          </a>
                      )
                  }
              )
          </div>
      </div>
  })
}
