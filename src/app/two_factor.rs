use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form, href, page, query_params, response::Response, route,
  },
  runtime::{expr, signal},
  view::{View, ViewExt, view},
};

use crate::auth::service::{
  self, clear_pending_2fa_cookie, portuguese_error_message,
  read_pending_2fa_cookie,
};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::field::{field, field_error, field_label};
use crate::components::input::input;
use crate::components::tabs::{tabs, tabs_content, tabs_list, tabs_trigger};
use crate::components::toast::{Toast, set_toast, toast_redirect};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct TwoFactorInput {
  code: Option<String>,
  backup_code: Option<String>,
}

#[query_params(error = bad_request)]
struct TwoFactorQuery {
  method: Option<String>,
}

// TODO: Review this page
#[page(GET "/two-factor")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<TwoFactorQuery>(cx)?;
  let is_backup = query.method.as_deref() == Some("backup");
  let has_pending = read_pending_2fa_cookie(cx).is_some();

  if !has_pending {
    return Ok(view! {
            container(
                variant: ContainerVariant::Centered,
                <div class="w-full">
                    <div class="mb-8 text-center">
                        <a href=(href!(crate::app::page)) class="text-2xl font-bold text-primary">"Linda Flor"</a>
                    </div>
                    card(
                        card_header(
                            card_title("Sessão expirada")
                            card_description(
                                "Faça login novamente para continuar a verificação em duas etapas."
                            )
                        )
                        card_footer(
                            <a
                                href=(href!(crate::app::login::page))
                                class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                            >
                                "Ir para o login"
                            </a>
                        )
                    )
                </div>
            )
        }
        .boxed());
  }

  let code = signal(cx, String::new);
  let code_touched = signal(cx, || false);
  let code_error = expr!({
    if !code_touched.get() {
      "".to_owned()
    } else if code.get().trim().is_empty() {
      "Informe o código de 6 dígitos.".to_owned()
    } else if code.get().trim().len() != 6.0 {
      "O código deve ter 6 dígitos.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let backup_code = signal(cx, String::new);
  let backup_code_touched = signal(cx, || false);
  let backup_code_error = expr!({
    if !backup_code_touched.get() {
      "".to_owned()
    } else if backup_code.get().trim().is_empty() {
      "Informe o código de backup.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let code_blocked = expr!({
    if code.get().trim().is_empty() {
      true
    } else if code.get().trim().len() != 6.0 {
      true
    } else {
      false
    }
  });
  let backup_blocked = expr!({ backup_code.get().trim().is_empty() });

  Ok(view! {
        container(
            variant: ContainerVariant::Centered,
            <div class="w-full">
                <div class="mb-8 text-center">
                    <a href=(href!(crate::app::page)) class="text-2xl font-bold text-primary">"Linda Flor"</a>
                </div>
                card(
                    card_header(
                        card_title("Verificação em duas etapas")
                        card_description(
                            "Digite o código do seu aplicativo autenticador ou use um código de backup."
                        )
                    )
                    card_content(
                        tabs(
                            tabs_list(
                                tabs_trigger(
                                    active: !is_backup,
                                    attrs: attributes! { href=(href!(page)) },
                                    "Autenticador"
                                )
                                tabs_trigger(
                                    active: is_backup,
                                    attrs: attributes! { href=(href!(page).query([("method", "backup")])) },
                                    "Código de backup"
                                )
                            )
                            tabs_content(
                                if is_backup {
                                    <form
                                        method="post"
                                        action=(href!(two_factor_post).query([("method", "backup")]))
                                        class="flex flex-col gap-4"
                                        novalidate=""
                                        data-toast-promise=""
                                        data-toast-loading="Verificando…"
                                    >
                                        field(
                                            attrs: attributes! {
                                                :data-invalid=$( (!backup_code_error.is_empty()).then_some("true") )
                                            },
                                            field_label(attrs: attributes! { for="backup_code" }, "Código de backup")
                                            input(
                                                value: backup_code,
                                                touched: backup_code_touched.clone(),
                                                error: backup_code_error.clone(),
                                                attrs: attributes! {
                                                    id="backup_code"
                                                    name="backup_code"
                                                    type="text"
                                                    placeholder="xxxx-xxxx"
                                                    autocomplete="off"
                                                    class="font-mono"
                                                    aria-describedby="backup_code-error"
                                                }
                                            )
                                            field_error(
                                                message: backup_code_error,
                                                attrs: attributes! { id="backup_code-error" }
                                            )
                                        )
                                        button(
                                            blocked: backup_blocked,
                                            attrs: attributes! { type="submit" },
                                            "Verificar"
                                        )
                                    </form>
                                } else {
                                    <form
                                        method="post"
                                        action=(href!(two_factor_post))
                                        class="flex flex-col gap-4"
                                        novalidate=""
                                        data-toast-promise=""
                                        data-toast-loading="Verificando…"
                                    >
                                        field(
                                            attrs: attributes! {
                                                :data-invalid=$( (!code_error.is_empty()).then_some("true") )
                                            },
                                            field_label(attrs: attributes! { for="code" }, "Código de 6 dígitos")
                                            input(
                                                value: code,
                                                touched: code_touched.clone(),
                                                error: code_error.clone(),
                                                attrs: attributes! {
                                                    id="code"
                                                    name="code"
                                                    type="text"
                                                    inputmode="numeric"
                                                    maxlength="6"
                                                    placeholder="000000"
                                                    autocomplete="one-time-code"
                                                    class="text-center text-xl tracking-widest font-mono"
                                                    aria-describedby="code-error"
                                                }
                                            )
                                            field_error(
                                                message: code_error,
                                                attrs: attributes! { id="code-error" }
                                            )
                                        )
                                        button(
                                            blocked: code_blocked,
                                            attrs: attributes! { type="submit" },
                                            "Verificar"
                                        )
                                    </form>
                                }
                            )
                        )
                    )
                    card_footer(
                        <a href=(href!(crate::app::login::page)) class="text-sm text-primary">
                            "Cancelar e fazer login novamente"
                        </a>
                    )
                )
            </div>
        )
    }.boxed())
}

fn two_factor_error_redirect(
  cx: &Cx,
  is_backup: bool,
  message: &str,
) -> Result<Response> {
  set_toast(cx, Toast::error(message));
  if is_backup {
    toast_redirect(cx, href!(page).query([("method", "backup")]).resolve(cx))
  } else {
    toast_redirect(cx, href!(page).resolve(cx))
  }
}

#[route(POST "/two-factor")]
pub async fn two_factor_post(
  cx: &Cx,
  Form(body): Form<TwoFactorInput>,
) -> Result<Response> {
  let pool = app_context::<PgPool>(cx);
  let Some(token) = read_pending_2fa_cookie(cx) else {
    return toast_redirect(cx, href!(crate::app::login::page).resolve(cx));
  };

  let is_backup = topcoat::router::request::uri(cx)
    .query()
    .is_some_and(|q| q.split('&').any(|p| p == "method=backup"));

  let result = if is_backup {
    let code = body.backup_code.as_deref().unwrap_or("");
    service::complete_2fa_with_backup(cx, pool, &token, code).await
  } else {
    let code = body.code.as_deref().unwrap_or("");
    service::complete_2fa_with_totp(cx, pool, &token, code).await
  };

  match result {
    Ok(_) => {
      clear_pending_2fa_cookie(cx);
      set_toast(cx, Toast::success("Verificado."));
      toast_redirect(cx, href!(crate::app::dashboard::page).resolve(cx))
    }
    Err(err) => {
      two_factor_error_redirect(cx, is_backup, &portuguese_error_message(&err))
    }
  }
}
