use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form, href, page, query_params, response::Response, route,
  },
  runtime::{expr, signal},
  view::{View, view},
};

use crate::auth::service::{self, MIN_PASSWORD_LEN, portuguese_error_message};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::field::{field, field_error, field_label};
use crate::components::input::input;
use crate::components::toast::{Toast, set_toast, toast_redirect};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct ResetInput {
  token: Option<String>,
  new_password: Option<String>,
  confirm_password: Option<String>,
}

#[query_params(error = bad_request)]
struct ResetQuery {
  token: Option<String>,
}

#[page(GET "/reset-password")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<ResetQuery>(cx)?;
  let token = query.token.clone().unwrap_or_default();
  let has_token = !token.is_empty();

  let min_password_len = MIN_PASSWORD_LEN as f64;
  let new_password = signal(cx, String::new);
  let new_password_touched = signal(cx, || false);
  let new_password_error = expr!({
    if !new_password_touched.get() {
      "".to_owned()
    } else if new_password.get().is_empty() {
      "Informe a nova senha.".to_owned()
    } else if new_password.get().len() < min_password_len {
      "A senha deve ter pelo menos 8 caracteres.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let confirm_password = signal(cx, String::new);
  let confirm_password_touched = signal(cx, || false);
  let confirm_password_error = expr!({
    if !confirm_password_touched.get() {
      "".to_owned()
    } else if confirm_password.get().is_empty() {
      "Confirme a nova senha.".to_owned()
    } else if confirm_password.get() != new_password.get() {
      "As senhas não conferem.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let submit_blocked = expr!({
    if new_password.get().is_empty() {
      true
    } else if new_password.get().len() < min_password_len {
      true
    } else if confirm_password.get().is_empty() {
      true
    } else if confirm_password.get() != new_password.get() {
      true
    } else {
      false
    }
  });

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
                              "O link de redefinição pode estar expirado ou já foi usado. Solicite um novo na página de login."
                          )
                      )
                      card_footer(
                          <a
                              href=(href!(crate::app::forgot_password::page))
                              class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                          >
                              "Solicitar novo link"
                          </a>
                      )
                  } else {
                      card_header(
                          card_title("Redefinir senha")
                          card_description("Escolha uma nova senha para sua conta.")
                      )
                      card_content(
                          <form
                                  method="post"
                                  action=(href!(reset_post))
                                  class="flex flex-col gap-4"
                                  novalidate=""
                                  data-toast-promise=""
                                  data-toast-loading="Redefinindo senha…">
                              <input type="hidden" name="token" value=(token)>
                              field(
                                  attrs: attributes! {
                                      :data-invalid=$( (!new_password_error.is_empty()).then_some("true") )
                                  },
                                  field_label(attrs: attributes! { for="new_password" }, "Nova senha")
                                  input(
                                      value: new_password,
                                      touched: new_password_touched.clone(),
                                      error: new_password_error.clone(),
                                      attrs: attributes! {
                                          id="new_password"
                                          name="new_password"
                                          type="password"
                                          autocomplete="new-password"
                                          aria-describedby="new_password-error"
                                      }
                                  )
                                  field_error(
                                      message: new_password_error,
                                      attrs: attributes! { id="new_password-error" }
                                  )
                              )
                              field(
                                  attrs: attributes! {
                                      :data-invalid=$( (!confirm_password_error.is_empty()).then_some("true") )
                                  },
                                  field_label(attrs: attributes! { for="confirm_password" }, "Confirmar nova senha")
                                  input(
                                      value: confirm_password,
                                      touched: confirm_password_touched.clone(),
                                      error: confirm_password_error.clone(),
                                      attrs: attributes! {
                                          id="confirm_password"
                                          name="confirm_password"
                                          type="password"
                                          autocomplete="new-password"
                                          aria-describedby="confirm_password-error"
                                      }
                                  )
                                  field_error(
                                      message: confirm_password_error,
                                      attrs: attributes! { id="confirm_password-error" }
                                  )
                              )
                              button(
                                  blocked: submit_blocked,
                                  attrs: attributes! { type="submit" },
                                  "Redefinir senha"
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

fn reset_error_redirect(cx: &Cx, token: &str, message: &str) -> Result<Response> {
  set_toast(cx, Toast::error(message));
  toast_redirect(cx, href!(page).query([("token", token)]).resolve(cx))
}

#[route(POST "/reset-password")]
pub async fn reset_post(
  cx: &Cx,
  Form(body): Form<ResetInput>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let token = body.token.as_deref().unwrap_or("").trim();
  let new_password = body.new_password.as_deref().unwrap_or("");
  let confirm = body.confirm_password.as_deref().unwrap_or("");

  if token.is_empty() {
    return toast_redirect(cx, href!(page).resolve(cx));
  }
  if new_password != confirm {
    return reset_error_redirect(cx, token, "As senhas não conferem.");
  }

  match service::reset_password(pool, token, new_password).await {
    Ok(()) => {
      set_toast(cx, Toast::success("Senha redefinida."));
      toast_redirect(
        cx,
        href!(crate::app::login::page)
          .query([("mode", "signin")])
          .resolve(cx),
      )
    }
    Err(err) => {
      reset_error_redirect(cx, token, &portuguese_error_message(&err))
    }
  }
}
