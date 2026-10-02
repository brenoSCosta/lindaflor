use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{content::Form, page, query_params, response::Response, route},
  view::{View, view},
};

use crate::components::button::{ButtonVariant, button};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::separator::separator;
use crate::components::toast::{Toast, set_toast, toast_redirect};
use lindaflor::auth::service::{
  self, SignInOutcome, portuguese_error_message, set_pending_2fa_cookie,
};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct LoginInput {
  name: Option<String>,
  email: Option<String>,
  password: Option<String>,
  mode: Option<String>,
  invite_id: Option<String>,
  next: Option<String>,
}

#[query_params(error = bad_request)]
struct LoginQuery {
  invite_id: Option<String>,
  email: Option<String>,
  mode: Option<String>,
  next: Option<String>,
}

fn safe_next(invite_id: Option<&str>, next: Option<&str>) -> String {
  if let Some(id) = invite_id.filter(|s| !s.is_empty()) {
    return format!("/accept-invitation?id={id}");
  }
  match next {
    Some(path) if path.starts_with('/') && !path.starts_with("//") => {
      path.to_owned()
    }
    _ => "/dashboard".to_string(),
  }
}

#[page(GET "/login")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<LoginQuery>(cx)?;
  let is_signup = query.mode.as_deref() == Some("signup");
  let default_email = query.email.clone().unwrap_or_default();
  let title = if is_signup {
    "Criar conta"
  } else {
    "Bem-vinda de volta"
  };
  let subtitle = if is_signup {
    "Cadastre-se para uma experiência personalizada."
  } else {
    "Entre para acompanhar seus pedidos e favoritos."
  };
  let submit_label = if is_signup { "Cadastrar" } else { "Entrar" };
  let switch_label = if is_signup { "Entrar" } else { "Cadastre-se" };
  let switch_url = if is_signup {
    "/login?mode=signin"
  } else {
    "/login?mode=signup"
  };
  let switch_prompt = if is_signup {
    "Já tem uma conta? "
  } else {
    "Precisa de uma conta? "
  };
  let mode_value = if is_signup { "signup" } else { "signin" };
  let loading_label = if is_signup {
    "Criando conta…"
  } else {
    "Entrando…"
  };
  let invite_id = query.invite_id.clone().unwrap_or_default();
  let next = query.next.clone().unwrap_or_default();

  Ok(view! {
      <div class="flex min-h-screen items-center justify-center bg-background px-4 py-8">
          <div class="w-full max-w-md">
              card(
                  card_header(
                      card_title((title))
                      card_description((subtitle))
                  )
                  card_content(
                      <form
                          method="post"
                          action="/login"
                          class="flex flex-col gap-4"
                          data-toast-promise=""
                          data-toast-loading=(loading_label)
                      >
                          <input type="hidden" name="mode" value=(mode_value)>
                          if !invite_id.is_empty() {
                              <input type="hidden" name="invite_id" value=(invite_id.clone())>
                          }
                          if !next.is_empty() {
                              <input type="hidden" name="next" value=(next.clone())>
                          }
                          if is_signup {
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="name" }, "Nome")
                                  input(attrs: attributes! { type="text" name="name" id="name" required="" })
                              </div>
                          }
                          <div class="space-y-2">
                              label(attrs: attributes! { for="email" }, "E-mail")
                              input(attrs: attributes! { type="email" name="email" id="email" value=(default_email) required="" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="password" }, "Senha")
                              input(attrs: attributes! { type="password" name="password" id="password" required="" })
                          </div>
                          if !is_signup {
                              <div class="text-right">
                                  <a href="/forgot-password" class="text-sm text-primary">
                                      "Esqueceu a senha?"
                                  </a>
                              </div>
                          }
                          button(
                              variant: ButtonVariant::Primary,
                              attrs: attributes! { type="submit" },
                              (submit_label)
                          )
                      </form>
                  )
                  card_footer(
                      <div class="flex flex-col items-center w-full gap-4">
                          <p class="text-sm text-muted-foreground">
                              (switch_prompt)
                              <a href=(switch_url) class="font-medium text-primary">
                                  (switch_label)
                              </a>
                          </p>
                          <div class="flex w-full items-center gap-4">
                              separator(attrs: attributes! { class="flex-1" })
                              <span class="text-xs text-muted-foreground">"ou continue com"</span>
                              separator(attrs: attributes! { class="flex-1" })
                          </div>
                          <div class="flex w-full flex-col gap-2">
                              <form method="post" action="/login/google" class="w-full">
                                  <input type="hidden" name="callback_url" value=(safe_next(query.invite_id.as_deref(), query.next.as_deref()))>
                                  button(
                                      variant: ButtonVariant::Outline,
                                      attrs: attributes! { type="submit" class="w-full" },
                                      "Google"
                                  )
                              </form>
                          </div>
                      </div>
                  )
              )
              <p class="mt-6 text-center text-xs text-muted-foreground">
                  "Ao continuar, você concorda com nossos "
                  <a href="/termos" class="text-primary">"Termos de Uso"</a>
                  " e "
                  <a href="/politica-privacidade" class="text-primary">"Política de Privacidade"</a>
                  "."
              </p>
          </div>
      </div>
  })
}

fn login_error_redirect(
  cx: &Cx,
  is_signup: bool,
  message: &str,
) -> Result<Response> {
  set_toast(cx, Toast::error(message));
  let mode = if is_signup { "signup" } else { "signin" };
  toast_redirect(cx, format!("/login?mode={mode}"))
}

#[route(POST "/login")]
pub async fn login_post(
  cx: &Cx,
  Form(body): Form<LoginInput>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let is_signup = body.mode.as_deref() == Some("signup");
  let next = safe_next(body.invite_id.as_deref(), body.next.as_deref());
  let email = body.email.as_deref().unwrap_or("");
  let password = body.password.as_deref().unwrap_or("");

  if is_signup {
    let name = body.name.as_deref().unwrap_or("");
    match service::sign_up_email(cx, pool, name, email, password).await {
      Ok(_) => {
        set_toast(cx, Toast::success("Conta criada."));
        toast_redirect(cx, next)
      }
      Err(err) => {
        login_error_redirect(cx, true, &portuguese_error_message(&err))
      }
    }
  } else {
    match service::sign_in_email(cx, pool, email, password).await {
      Ok(SignInOutcome::SignedIn(_)) => {
        set_toast(cx, Toast::success("Bem-vinda de volta."));
        toast_redirect(cx, next)
      }
      Ok(SignInOutcome::TwoFactorRequired { token }) => {
        set_pending_2fa_cookie(cx, &token);
        toast_redirect(cx, "/two-factor")
      }
      Err(err) => {
        login_error_redirect(cx, false, &portuguese_error_message(&err))
      }
    }
  }
}

#[derive(Deserialize)]
pub struct GoogleLoginInput {
  callback_url: Option<String>,
}

#[route(POST "/login/google")]
pub async fn login_google(
  cx: &Cx,
  Form(body): Form<GoogleLoginInput>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  match service::start_google_oauth(pool, body.callback_url.as_deref(), None)
    .await
  {
    Ok(url) => toast_redirect(cx, url),
    Err(err) => {
      login_error_redirect(cx, false, &portuguese_error_message(&err))
    }
  }
}
