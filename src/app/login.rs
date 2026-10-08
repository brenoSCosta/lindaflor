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

use crate::auth::service::{
  self, MIN_PASSWORD_LEN, SignInOutcome, portuguese_error_message,
  set_pending_2fa_cookie,
};
use crate::components::button::{ButtonVariant, button};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::field::{field, field_error, field_label};
use crate::components::input::input;
use crate::components::separator::separator;
use crate::components::toast::{Toast, set_toast, toast_redirect};
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

fn safe_next(cx: &Cx, invite_id: Option<&str>, next: Option<&str>) -> String {
  if let Some(id) = invite_id.filter(|s| !s.is_empty()) {
    return format!("/accept-invitation?id={id}");
  }
  match next {
    Some(path) if path.starts_with('/') && !path.starts_with("//") => {
      path.to_owned()
    }
    _ => href!(crate::app::dashboard::page).resolve(cx),
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
    href!(page).query([("mode", "signin")])
  } else {
    href!(page).query([("mode", "signup")])
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

  let min_password_len = MIN_PASSWORD_LEN as f64;
  let name = signal(cx, String::new);
  let name_touched = signal(cx, || false);
  let name_error = expr!({
    if !name_touched.get() {
      "".to_owned()
    } else if name.get().trim().is_empty() {
      "Informe o nome.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let email = signal(cx, || default_email.clone());
  let email_touched = signal(cx, || false);
  let email_error = expr!({
    if !email_touched.get() {
      "".to_owned()
    } else if email.get().trim().is_empty() {
      "Informe o e-mail.".to_owned()
    } else if !email.get().contains("@") {
      "E-mail inválido.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let password = signal(cx, String::new);
  let password_touched = signal(cx, || false);
  let password_error = expr!({
    if !password_touched.get() {
      "".to_owned()
    } else if password.get().is_empty() {
      "Informe a senha.".to_owned()
    } else if password.get().len() < min_password_len {
      "A senha deve ter pelo menos 8 caracteres.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let submit_blocked = expr!({
    if is_signup {
      if name.get().trim().is_empty() {
        true
      } else if email.get().trim().is_empty() {
        true
      } else if !email.get().contains("@") {
        true
      } else if password.get().is_empty() {
        true
      } else if password.get().len() < min_password_len {
        true
      } else {
        false
      }
    } else if email.get().trim().is_empty() {
      true
    } else if !email.get().contains("@") {
      true
    } else if password.get().is_empty() {
      true
    } else if password.get().len() < min_password_len {
      true
    } else {
      false
    }
  });

  Ok(view! {
      container(
          variant: ContainerVariant::Centered,
              card(
                  card_header(
                      card_title((title))
                      card_description((subtitle))
                  )
                  card_content(
                      <form
                              method="post"
                              action=(href!(login_post))
                              class="flex flex-col gap-4"
                              novalidate=""
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
                              field(
                                  attrs: attributes! {
                                      :data-invalid=$( (!name_error.is_empty()).then_some("true") )
                                  },
                                  field_label(attrs: attributes! { for="name" }, "Nome")
                                  input(
                                      value: name,
                                      touched: name_touched.clone(),
                                      error: name_error.clone(),
                                      attrs: attributes! {
                                          id="name"
                                          name="name"
                                          type="text"
                                          autocomplete="name"
                                          aria-describedby="name-error"
                                      }
                                  )
                                  field_error(
                                      message: name_error,
                                      attrs: attributes! { id="name-error" }
                                  )
                              )
                          }
                          field(
                              attrs: attributes! {
                                  :data-invalid=$( (!email_error.is_empty()).then_some("true") )
                              },
                              field_label(attrs: attributes! { for="email" }, "E-mail")
                              input(
                                  value: email,
                                  touched: email_touched.clone(),
                                  error: email_error.clone(),
                                  attrs: attributes! {
                                      id="email"
                                      name="email"
                                      type="email"
                                      autocomplete="email"
                                      aria-describedby="email-error"
                                  }
                              )
                              field_error(
                                  message: email_error,
                                  attrs: attributes! { id="email-error" }
                              )
                          )
                          field(
                              attrs: attributes! {
                                  :data-invalid=$( (!password_error.is_empty()).then_some("true") )
                              },
                              field_label(attrs: attributes! { for="password" }, "Senha")
                              input(
                                  value: password,
                                  touched: password_touched.clone(),
                                  error: password_error.clone(),
                                  attrs: attributes! {
                                      id="password"
                                      name="password"
                                      type="password"
                                      autocomplete="current-password"
                                      aria-describedby="password-error"
                                  }
                              )
                              field_error(
                                  message: password_error,
                                  attrs: attributes! { id="password-error" }
                              )
                          )
                          if !is_signup {
                              <div class="text-right">
                                  <a href=(href!(crate::app::forgot_password::page)) class="text-sm text-primary">
                                      "Esqueceu a senha?"
                                  </a>
                              </div>
                          }
                          button(
                              blocked: submit_blocked,
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
                              <form
                                  method="post"
                                  action=(href!(login_google))
                                  class="w-full"
                              >
                                  <input type="hidden" name="callback_url" value=(safe_next(cx, query.invite_id.as_deref(), query.next.as_deref()))>
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
                  <a href=(href!(crate::app::termos::page)) class="text-primary">"Termos de Uso"</a>
                  " e "
                  <a href=(href!(crate::app::politica_privacidade::page)) class="text-primary">"Política de Privacidade"</a>
                  "."
              </p>
      )
  })
}

fn login_error_redirect(
  cx: &Cx,
  is_signup: bool,
  message: &str,
) -> Result<Response> {
  set_toast(cx, Toast::error(message));
  let mode = if is_signup { "signup" } else { "signin" };
  toast_redirect(cx, href!(page).query([("mode", mode)]).resolve(cx))
}

#[route(POST "/login")]
pub async fn login_post(
  cx: &Cx,
  Form(body): Form<LoginInput>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let is_signup = body.mode.as_deref() == Some("signup");
  let next = safe_next(cx, body.invite_id.as_deref(), body.next.as_deref());
  let email = body.email.as_deref().unwrap_or("");
  let password = body.password.as_deref().unwrap_or("");

  let (scope, default) = if is_signup {
    (
      crate::rate_limit::SCOPE_SIGN_UP,
      crate::rate_limit::DEFAULT_SIGN_UP_PER_MIN,
    )
  } else {
    (
      crate::rate_limit::SCOPE_SIGN_IN,
      crate::rate_limit::DEFAULT_SIGN_IN_PER_MIN,
    )
  };
  if !crate::rate_limit::check_ip_rate(
    scope,
    &crate::valkey::client_ip(cx),
    crate::rate_limit::limit_for(scope, default),
  ) {
    return login_error_redirect(
      cx,
      is_signup,
      "Muitas tentativas. Aguarde um momento e tente novamente.",
    );
  }

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
        toast_redirect(cx, href!(crate::app::two_factor::page).resolve(cx))
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
  match service::start_google_oauth(
    pool,
    body.callback_url.as_deref(),
    None,
    None,
  )
  .await
  {
    Ok(url) => toast_redirect(cx, url),
    Err(err) => {
      login_error_redirect(cx, false, &portuguese_error_message(&err))
    }
  }
}
