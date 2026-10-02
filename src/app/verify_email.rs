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
use lindaflor::auth::service;
use lindaflor::auth::user::current_user_owned;
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct ResendInput {
  email: Option<String>,
}

#[query_params(error = bad_request)]
struct VerifyQuery {
  token: Option<String>,
  error: Option<String>,
}

#[page(GET "/verify-email")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<VerifyQuery>(cx)?;
  let pool = app_context::<PgPool>(cx);

  let mut verified = false;
  let mut has_error = query.error.is_some();

  if let Some(token) = query.token.as_deref().filter(|t| !t.is_empty()) {
    match service::consume_email_verification_token(pool, token).await {
      Ok(()) => verified = true,
      Err(_) => has_error = true,
    }
  }

  let resent = query.error.as_deref() == Some("resent");
  // When `?error=resent` we show success for resend, not failure.
  if resent {
    has_error = false;
  }

  let session_email = current_user_owned(cx)
    .await?
    .map(|su| su.user.email)
    .unwrap_or_default();

  Ok(view! {
      <div class="flex min-h-screen items-center justify-center bg-background px-4 py-8">
          <div class="w-full max-w-md">
              <div class="mb-8 text-center">
                  <a href="/" class="text-2xl font-bold text-primary">"Linda Flor"</a>
              </div>
              card(
                  if has_error {
                      card_header(
                          card_title("Falha na verificação")
                          card_description(
                              "O link pode estar expirado ou já foi usado. Solicite um novo abaixo."
                          )
                      )
                      card_content(
                          <form method="post" action="/verify-email/resend" class="flex flex-col gap-4">
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="email" }, "E-mail")
                                  input(attrs: attributes! {
                                      type="email"
                                      name="email"
                                      id="email"
                                      value=(session_email)
                                      required=""
                                  })
                              </div>
                              button(
                                  variant: ButtonVariant::Primary,
                                  attrs: attributes! { type="submit" },
                                  "Reenviar verificação"
                              )
                          </form>
                      )
                      card_footer(
                          <a
                              href="/login"
                              class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                          >
                              "Voltar para o login"
                          </a>
                      )
                  } else if verified || resent {
                      card_header(
                          card_title(
                              if resent { "Link reenviado" } else { "E-mail verificado" }
                          )
                          card_description(
                              if resent {
                                  "Enviamos um novo link de verificação para o seu e-mail."
                              } else {
                                  "Seu e-mail foi confirmado com sucesso."
                              }
                          )
                      )
                      card_footer(
                          <a
                              href="/dashboard"
                              class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                          >
                              "Ir para o painel"
                          </a>
                      )
                  } else {
                      card_header(
                          card_title("Verificar e-mail")
                          card_description(
                              "Abra o link enviado ao seu e-mail, ou solicite um novo abaixo."
                          )
                      )
                      card_content(
                          <form method="post" action="/verify-email/resend" class="flex flex-col gap-4">
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="email" }, "E-mail")
                                  input(attrs: attributes! {
                                      type="email"
                                      name="email"
                                      id="email"
                                      value=(session_email)
                                      required=""
                                  })
                              </div>
                              button(
                                  variant: ButtonVariant::Primary,
                                  attrs: attributes! { type="submit" },
                                  "Reenviar verificação"
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

#[route(POST "/verify-email/resend")]
pub async fn resend(
  cx: &Cx,
  Form(body): Form<ResendInput>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  let email = if let Some(email) =
    body.email.as_deref().filter(|e| !e.trim().is_empty())
  {
    email.to_owned()
  } else if let Some(su) = current_user_owned(cx).await? {
    su.user.email
  } else {
    return Ok(see_other("/verify-email?error=1"));
  };

  let _ =
    service::send_verification_email(pool, &email, Some("/verify-email")).await;
  Ok(see_other("/verify-email?error=resent"))
}
