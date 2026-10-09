use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form, href, page, query_params, response::Response, route,
  },
  runtime::{expr, signal},
  view::{View, component, view},
};

use crate::auth::service::{
  self, MIN_PASSWORD_LEN, SignInOutcome, portuguese_error_message,
  set_pending_2fa_cookie,
};
use crate::components::button::{ButtonVariant, button};
use crate::components::field::{
  field, field_error, field_label, field_separator,
};
use crate::components::input::input;
use crate::components::toast::{Toast, set_toast, toast_redirect};
use topcoat::icon::{icon, iconify::iconify_icon};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct SignInInput {
  email: Option<String>,
  password: Option<String>,
  invite_id: Option<String>,
  next: Option<String>,
}

#[derive(Deserialize)]
pub struct SignUpInput {
  name: Option<String>,
  email: Option<String>,
  password: Option<String>,
  invite_id: Option<String>,
  next: Option<String>,
}

#[query_params(error = bad_request)]
struct LoginQuery {
  invite_id: Option<String>,
  email: Option<String>,
  name: Option<String>,
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

#[component]
async fn signin_form(
  cx: &Cx,
  default_email: String,
  invite_id: String,
  next: String,
) -> Result<impl View> {
  // `len()` on strings is f64 in expr vocabulary (JS number
  // semantics), so keep the threshold as f64 for same-type comparison.
  let min_len = MIN_PASSWORD_LEN as f64;
  let email = signal(cx, || default_email.clone());
  let email_touched = signal(cx, || false);
  let email_error = expr!({
    let touched = email_touched.get();
    let value = email.get();
    if !touched {
      "".to_owned()
    } else if value.trim().is_empty() {
      "Informe o e-mail.".to_owned()
    } else if !value.contains("@") {
      "E-mail inválido.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let password = signal(cx, String::new);
  let password_touched = signal(cx, || false);
  let password_error = expr!({
    let touched = password_touched.get();
    let value = password.get();
    if !touched {
      "".to_owned()
    } else if value.is_empty() {
      "Informe a senha.".to_owned()
    } else if value.len() < min_len {
      "A senha deve ter pelo menos 8 caracteres.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let signin_blocked = expr!({
    let email_val = email.get();
    let password_val = password.get();
    if email_val.trim().is_empty() {
      true
    } else if !email_val.contains("@") {
      true
    } else if password_val.is_empty() {
      true
    } else if password_val.len() < min_len {
      true
    } else {
      false
    }
  });

  Ok(view! {
      <form
          method="post"
          action=(href!(signin_post))
          class="flex flex-col gap-4"
          novalidate=""
          data-toast-promise=""
          data-toast-loading="Entrando…"
      >
          if !invite_id.is_empty() {
              <input type="hidden" name="invite_id" value=(invite_id.clone())>
          }
          if !next.is_empty() {
              <input type="hidden" name="next" value=(next.clone())>
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
                      placeholder="E-mail"
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
                      placeholder="Senha"
                      aria-describedby="password-error"
                  }
              )
              field_error(
                  message: password_error,
                  attrs: attributes! { id="password-error" }
              )
          )
          button(
              blocked: signin_blocked,
              attrs: attributes! { type="submit" class="w-full" },
              "Continuar com e-mail"
          )
          <a
              href=(href!(crate::app::forgot_password::page))
              class="block text-center text-sm text-muted-foreground underline-offset-4 hover:underline"
          >
              "Esqueceu a senha?"
          </a>
      </form>
  })
}

#[component]
async fn signup_form(
  cx: &Cx,
  default_name: String,
  default_email: String,
  invite_id: String,
  next: String,
) -> Result<impl View> {
  let min_len = MIN_PASSWORD_LEN as f64;
  let name = signal(cx, || default_name.clone());
  let name_touched = signal(cx, || false);
  let name_error = expr!({
    let touched = name_touched.get();
    let value = name.get();
    if !touched {
      "".to_owned()
    } else if value.trim().is_empty() {
      "Informe o nome.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let email = signal(cx, || default_email.clone());
  let email_touched = signal(cx, || false);
  let email_error = expr!({
    let touched = email_touched.get();
    let value = email.get();
    if !touched {
      "".to_owned()
    } else if value.trim().is_empty() {
      "Informe o e-mail.".to_owned()
    } else if !value.contains("@") {
      "E-mail inválido.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let password = signal(cx, String::new);
  let password_touched = signal(cx, || false);
  let password_error = expr!({
    let touched = password_touched.get();
    let value = password.get();
    if !touched {
      "".to_owned()
    } else if value.is_empty() {
      "Informe a senha.".to_owned()
    } else if value.len() < min_len {
      "A senha deve ter pelo menos 8 caracteres.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let signup_blocked = expr!({
    let name_val = name.get();
    let email_val = email.get();
    let password_val = password.get();
    if name_val.trim().is_empty() {
      true
    } else if email_val.trim().is_empty() {
      true
    } else if !email_val.contains("@") {
      true
    } else if password_val.is_empty() {
      true
    } else if password_val.len() < min_len {
      true
    } else {
      false
    }
  });

  Ok(view! {
      <form
          method="post"
          action=(href!(signup_post))
          class="flex flex-col gap-4"
          novalidate=""
          data-toast-promise=""
          data-toast-loading="Criando conta…"
      >
          if !invite_id.is_empty() {
              <input type="hidden" name="invite_id" value=(invite_id.clone())>
          }
          if !next.is_empty() {
              <input type="hidden" name="next" value=(next.clone())>
          }
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
                      placeholder="Nome"
                      aria-describedby="name-error"
                  }
              )
              field_error(
                  message: name_error,
                  attrs: attributes! { id="name-error" }
              )
          )
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
                      placeholder="E-mail"
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
                      autocomplete="new-password"
                      placeholder="Senha"
                      aria-describedby="password-error"
                  }
              )
              field_error(
                  message: password_error,
                  attrs: attributes! { id="password-error" }
              )
          )
          button(
              blocked: signup_blocked,
              attrs: attributes! { type="submit" class="w-full" },
              "Continuar com e-mail"
          )
      </form>
  })
}

#[page(GET "/login")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<LoginQuery>(cx)?;
  let is_signup = query.mode.as_deref() == Some("signup");
  let default_email = query.email.clone().unwrap_or_default();
  let default_name = query.name.clone().unwrap_or_default();
  let title = if is_signup {
    "Cadastre-se na Linda Flor"
  } else {
    "Entre na Linda Flor"
  };
  let switch_label = if is_signup { "Entrar" } else { "Criar conta" };
  let switch_url = if is_signup {
    href!(page).query([("mode", "signin")])
  } else {
    href!(page).query([("mode", "signup")])
  };
  let switch_prompt = if is_signup {
    "Já tem uma conta? "
  } else {
    "Não tem uma conta? "
  };
  let invite_id = query.invite_id.clone().unwrap_or_default();
  let next = query.next.clone().unwrap_or_default();
  let callback_url =
    safe_next(cx, query.invite_id.as_deref(), query.next.as_deref());
  let hero_src = if is_signup {
    "https://images.unsplash.com/photo-1519046904884-53103b34b206?auto=format&fit=crop&w=1600&q=80"
  } else {
    "https://images.unsplash.com/photo-1507525428034-b723cf961d3e?auto=format&fit=crop&w=1600&q=80"
  };
  let hero_alt = if is_signup {
    "Saída de praia"
  } else {
    "Mar e horizonte"
  };

  Ok(view! {
      <div class="flex min-h-full flex-1">
          if is_signup {
              <div class="relative hidden bg-muted lg:block lg:w-1/2">
                  <img
                      src=(hero_src)
                      alt=(hero_alt)
                      class="absolute inset-0 h-full w-full object-cover dark:brightness-[0.9]"
                  >
              </div>
          }
          <div class="flex w-full items-center justify-center p-6 md:p-10 lg:w-1/2">
              <div class="relative w-full max-w-xs">
                  // Divs para criar o efeito de borda
                  <div class="pointer-events-none absolute inset-x-0 top-0 w-[calc(100%+4rem)] -translate-x-8 border-t max-sm:hidden"></div>
                  <div class="pointer-events-none absolute inset-x-0 bottom-0 w-[calc(100%+4rem)] -translate-x-8 border-b max-sm:hidden"></div>
                  <div class="pointer-events-none absolute inset-y-0 left-0 h-[calc(100%+4rem)] -translate-y-8 border-s max-sm:hidden"></div>
                  <div class="pointer-events-none absolute inset-y-0 right-0 h-[calc(100%+4rem)] -translate-y-8 border-e max-sm:hidden"></div>
                  <div class="pointer-events-none absolute inset-x-0 -top-1 w-[calc(100%+3rem)] -translate-x-6 border-t max-sm:hidden"></div>
                  <div class="pointer-events-none absolute inset-x-0 -bottom-1 w-[calc(100%+3rem)] -translate-x-6 border-b max-sm:hidden"></div>
                  <div class="pointer-events-none absolute inset-y-0 -left-1 h-[calc(100%+3rem)] -translate-y-6 border-s max-sm:hidden"></div>
                  <div class="pointer-events-none absolute inset-y-0 -right-1 h-[calc(100%+3rem)] -translate-y-6 border-e max-sm:hidden"></div>

                  <div class="flex w-full flex-col gap-6 p-8">
                  <div class="flex flex-col items-center gap-2 text-center">
                      icon(
                          data: iconify_icon!("lucide:flower-2"),
                          attrs: attributes! { class="size-8 text-primary" }
                      )
                      <h1 class="text-2xl font-bold tracking-tight">(title)</h1>
                  </div>
                  <form
                      method="post"
                      action=(href!(login_google))
                      class="w-full"
                  >
                      <input type="hidden" name="callback_url" value=(callback_url)>
                      button(
                          variant: ButtonVariant::Outline,
                          attrs: attributes! { type="submit" class="w-full" },
                          "Continuar com Google"
                      )
                  </form>
                  field_separator("OU")
                  if is_signup {
                      signup_form(
                          default_name: default_name.clone(),
                          default_email: default_email.clone(),
                          invite_id: invite_id.clone(),
                          next: next.clone(),
                      )
                  } else {
                      signin_form(
                          default_email: default_email.clone(),
                          invite_id: invite_id.clone(),
                          next: next.clone(),
                      )
                  }
                  <p class="text-center text-sm text-muted-foreground">
                      (switch_prompt)
                      <a href=(switch_url) class="underline underline-offset-4 hover:text-foreground">
                          (switch_label)
                      </a>
                  </p>
                  <p class="text-center text-xs text-muted-foreground">
                      "Ao continuar, você concorda com nossos "
                      <a href=(href!(crate::app::termos::page)) class="underline underline-offset-4 hover:text-foreground">"Termos de Uso"</a>
                      " e "
                      <a href=(href!(crate::app::politica_privacidade::page)) class="underline underline-offset-4 hover:text-foreground">"Política de Privacidade"</a>
                      "."
                  </p>
                  </div>
              </div>
          </div>
          if !is_signup {
              <div class="relative hidden bg-muted lg:block lg:w-1/2">
                  <img
                      src=(hero_src)
                      alt=(hero_alt)
                      class="absolute inset-0 h-full w-full object-cover dark:brightness-[0.9]"
                  >
              </div>
          }
      </div>
  })
}

fn signin_error_redirect(
  cx: &Cx,
  email: &str,
  invite_id: Option<&str>,
  next: Option<&str>,
  message: &str,
) -> Result<Response> {
  set_toast(cx, Toast::error(message));
  let mut params: Vec<(&str, &str)> = vec![("mode", "signin")];
  if !email.is_empty() {
    params.push(("email", email));
  }
  if let Some(id) = invite_id.filter(|s| !s.is_empty()) {
    params.push(("invite_id", id));
  }
  if let Some(path) = next.filter(|s| !s.is_empty()) {
    params.push(("next", path));
  }
  toast_redirect(cx, href!(page).query(params).resolve(cx))
}

fn signup_error_redirect(
  cx: &Cx,
  name: &str,
  email: &str,
  invite_id: Option<&str>,
  next: Option<&str>,
  message: &str,
) -> Result<Response> {
  set_toast(cx, Toast::error(message));
  let mut params: Vec<(&str, &str)> = vec![("mode", "signup")];
  if !name.is_empty() {
    params.push(("name", name));
  }
  if !email.is_empty() {
    params.push(("email", email));
  }
  if let Some(id) = invite_id.filter(|s| !s.is_empty()) {
    params.push(("invite_id", id));
  }
  if let Some(path) = next.filter(|s| !s.is_empty()) {
    params.push(("next", path));
  }
  toast_redirect(cx, href!(page).query(params).resolve(cx))
}

#[route(POST "/login")]
pub async fn signin_post(
  cx: &Cx,
  Form(body): Form<SignInInput>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let next = safe_next(cx, body.invite_id.as_deref(), body.next.as_deref());
  let email = body.email.as_deref().unwrap_or("");
  let password = body.password.as_deref().unwrap_or("");

  if !crate::rate_limit::check_ip_rate(
    crate::rate_limit::SCOPE_SIGN_IN,
    &crate::valkey::client_ip(cx),
    crate::rate_limit::limit_for(
      crate::rate_limit::SCOPE_SIGN_IN,
      crate::rate_limit::DEFAULT_SIGN_IN_PER_MIN,
    ),
  ) {
    return signin_error_redirect(
      cx,
      email,
      body.invite_id.as_deref(),
      body.next.as_deref(),
      "Muitas tentativas. Aguarde um momento e tente novamente.",
    );
  }

  match service::sign_in_email(cx, pool, email, password).await {
    Ok(SignInOutcome::SignedIn(_)) => {
      set_toast(cx, Toast::success("Bem-vinda de volta."));
      toast_redirect(cx, next)
    }
    Ok(SignInOutcome::TwoFactorRequired { token }) => {
      set_pending_2fa_cookie(cx, &token);
      toast_redirect(cx, href!(crate::app::two_factor::page).resolve(cx))
    }
    Err(err) => signin_error_redirect(
      cx,
      email,
      body.invite_id.as_deref(),
      body.next.as_deref(),
      &portuguese_error_message(&err),
    ),
  }
}

#[route(POST "/signup")]
pub async fn signup_post(
  cx: &Cx,
  Form(body): Form<SignUpInput>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let next = safe_next(cx, body.invite_id.as_deref(), body.next.as_deref());
  let name = body.name.as_deref().unwrap_or("");
  let email = body.email.as_deref().unwrap_or("");
  let password = body.password.as_deref().unwrap_or("");

  if !crate::rate_limit::check_ip_rate(
    crate::rate_limit::SCOPE_SIGN_UP,
    &crate::valkey::client_ip(cx),
    crate::rate_limit::limit_for(
      crate::rate_limit::SCOPE_SIGN_UP,
      crate::rate_limit::DEFAULT_SIGN_UP_PER_MIN,
    ),
  ) {
    return signup_error_redirect(
      cx,
      name,
      email,
      body.invite_id.as_deref(),
      body.next.as_deref(),
      "Muitas tentativas. Aguarde um momento e tente novamente.",
    );
  }

  match service::sign_up_email(cx, pool, name, email, password).await {
    Ok(_) => {
      set_toast(cx, Toast::success("Conta criada."));
      toast_redirect(cx, next)
    }
    Err(err) => signup_error_redirect(
      cx,
      name,
      email,
      body.invite_id.as_deref(),
      body.next.as_deref(),
      &portuguese_error_message(&err),
    ),
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
      signin_error_redirect(cx, "", None, None, &portuguese_error_message(&err))
    }
  }
}
