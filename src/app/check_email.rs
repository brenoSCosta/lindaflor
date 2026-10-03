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
use lindaflor::auth::service;
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
                                      <strong>(email.clone())</strong>
                                      ". Clique no link para concluir o login."
                                  </span>
                              }
                          )
                      )
                      card_content(
                          <form method="post" action="/check-email/resend">
                              <input type="hidden" name="email" value=(email.clone())>
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
    return Ok(see_other("/check-email"));
  };

  let _ =
    service::send_verification_email(pool, &email, Some("/verify-email")).await;
  Ok(see_other(format!(
    "/check-email?email={}&resent=1",
    percent_encode(&email)
  )))
}

fn percent_encode(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  for byte in input.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
        out.push(byte as char);
      }
      _ => {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        out.push('%');
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
      }
    }
  }
  out
}
