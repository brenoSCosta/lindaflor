use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    page, query_params, route,
  },
  view::{View, view},
};

use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::input::input;
use crate::components::label::label;
use lindaflor::auth::service::{self, portuguese_error_message};
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
  error: Option<String>,
}

#[page(GET "/reset-password")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<ResetQuery>(cx)?;
  let token = query.token.clone().unwrap_or_default();
  let has_token = !token.is_empty();
  let error_message = query.error.clone();

  Ok(view! {
      <div class="flex min-h-screen items-center justify-center bg-background px-4 py-8">
          <div class="w-full max-w-md">
              <div class="mb-8 text-center">
                  <a href="/" class="text-2xl font-bold text-primary">"Linda Flor"</a>
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
                              href="/forgot-password"
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
                          if let Some(ref msg) = error_message {
                              <div class="mb-4 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                                  (msg.as_str())
                              </div>
                          }
                          <form method="post" action="/reset-password" class="flex flex-col gap-4">
                              <input type="hidden" name="token" value=(token)>
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="new_password" }, "Nova senha")
                                  input(attrs: attributes! { type="password" name="new_password" id="new_password" required="" minlength="8" })
                              </div>
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="confirm_password" }, "Confirmar nova senha")
                                  input(attrs: attributes! { type="password" name="confirm_password" id="confirm_password" required="" minlength="8" })
                              </div>
                              button(
                                  variant: ButtonVariant::Primary,
                                  attrs: attributes! { type="submit" },
                                  "Redefinir senha"
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

fn encode_query(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        out.push(b as char)
      }
      b' ' => out.push_str("%20"),
      _ => out.push_str(&format!("%{b:02X}")),
    }
  }
  out
}

#[route(POST "/reset-password")]
pub async fn reset_post(
  cx: &Cx,
  Form(body): Form<ResetInput>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  let token = body.token.as_deref().unwrap_or("").trim();
  let new_password = body.new_password.as_deref().unwrap_or("");
  let confirm = body.confirm_password.as_deref().unwrap_or("");

  if token.is_empty() {
    return Ok(see_other("/reset-password"));
  }
  if new_password != confirm {
    return Ok(see_other(format!(
      "/reset-password?token={}&error={}",
      encode_query(token),
      encode_query("As senhas não conferem.")
    )));
  }

  match service::reset_password(pool, token, new_password).await {
    Ok(()) => Ok(see_other("/login?mode=signin")),
    Err(err) => Ok(see_other(format!(
      "/reset-password?token={}&error={}",
      encode_query(token),
      encode_query(&portuguese_error_message(&err))
    ))),
  }
}
