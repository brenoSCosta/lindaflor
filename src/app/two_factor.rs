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
  view::{View, ViewExt, view},
};

use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_description, card_footer, card_header, card_title,
};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::switch::switch;
use crate::components::tabs::{tabs, tabs_content, tabs_list, tabs_trigger};
use lindaflor::auth::service::{
  self, clear_pending_2fa_cookie, portuguese_error_message,
  read_pending_2fa_cookie,
};
use topcoat::view::attributes;

#[derive(Deserialize)]
pub struct TwoFactorInput {
  code: Option<String>,
  backup_code: Option<String>,
  trust_device: Option<String>,
}

#[query_params(error = bad_request)]
struct TwoFactorQuery {
  method: Option<String>,
  error: Option<String>,
}

#[page(GET "/two-factor")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<TwoFactorQuery>(cx)?;
  let is_backup = query.method.as_deref() == Some("backup");
  let error_message = query.error.clone();
  let has_pending = read_pending_2fa_cookie(cx).is_some();

  if !has_pending {
    return Ok(view! {
            <div class="flex min-h-screen items-center justify-center bg-background px-4 py-8">
                <div class="w-full max-w-md">
                    <div class="mb-8 text-center">
                        <a href="/" class="text-2xl font-bold text-primary">"Linda Flor"</a>
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
                                href="/login"
                                class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                            >
                                "Ir para o login"
                            </a>
                        )
                    )
                </div>
            </div>
        }
        .boxed());
  }

  Ok(view! {
        <div class="flex min-h-screen items-center justify-center bg-background px-4 py-8">
            <div class="w-full max-w-md">
                <div class="mb-8 text-center">
                    <a href="/" class="text-2xl font-bold text-primary">"Linda Flor"</a>
                </div>
                card(
                    card_header(
                        card_title("Verificação em duas etapas")
                        card_description(
                            "Digite o código do seu aplicativo autenticador ou use um código de backup."
                        )
                    )
                    card_content(
                        if let Some(ref msg) = error_message {
                            <div class="mb-4 rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                                (msg.as_str())
                            </div>
                        }
                        tabs(
                            tabs_list(
                                tabs_trigger(
                                    active: !is_backup,
                                    attrs: attributes! { href="/two-factor" },
                                    "Autenticador"
                                )
                                tabs_trigger(
                                    active: is_backup,
                                    attrs: attributes! { href="/two-factor?method=backup" },
                                    "Código de backup"
                                )
                            )
                            tabs_content(
                                if is_backup {
                                    <form method="post" action="/two-factor?method=backup" class="flex flex-col gap-4">
                                        <div class="space-y-2">
                                            label(attrs: attributes! { for="backup_code" }, "Código de backup")
                                            input(attrs: attributes! { type="text" name="backup_code" id="backup_code" placeholder="xxxx-xxxx" autocomplete="off" class="font-mono" })
                                        </div>
                                        <div class="flex items-center gap-2">
                                            switch(attrs: attributes! { type="checkbox" name="trust_device" id="trust_device" value="true" })
                                            label(attrs: attributes! { for="trust_device" }, "Confiar neste dispositivo por 30 dias")
                                        </div>
                                        button(
                                            variant: ButtonVariant::Primary,
                                            attrs: attributes! { type="submit" },
                                            "Verificar"
                                        )
                                    </form>
                                } else {
                                    <form method="post" action="/two-factor" class="flex flex-col gap-4">
                                        <div class="space-y-2">
                                            label(attrs: attributes! { for="code" }, "Código de 6 dígitos")
                                            input(attrs: attributes! { type="text" name="code" id="code" inputmode="numeric" maxlength="6" placeholder="000000" autocomplete="one-time-code" class="text-center text-xl tracking-widest font-mono" })
                                        </div>
                                        <div class="flex items-center gap-2">
                                            switch(attrs: attributes! { type="checkbox" name="trust_device" id="trust_device" value="true" })
                                            label(attrs: attributes! { for="trust_device" }, "Confiar neste dispositivo por 30 dias")
                                        </div>
                                        button(
                                            variant: ButtonVariant::Primary,
                                            attrs: attributes! { type="submit" },
                                            "Verificar"
                                        )
                                    </form>
                                }
                            )
                        )
                    )
                    card_footer(
                        <a href="/login" class="text-sm text-primary">
                            "Cancelar e fazer login novamente"
                        </a>
                    )
                )
            </div>
        </div>
    }.boxed())
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

#[route(POST "/two-factor")]
pub async fn two_factor_post(
  cx: &Cx,
  Form(body): Form<TwoFactorInput>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  let Some(token) = read_pending_2fa_cookie(cx) else {
    return Ok(see_other("/login"));
  };

  let is_backup = topcoat::router::request::uri(cx)
    .query()
    .is_some_and(|q| q.split('&').any(|p| p == "method=backup"));
  let _ = body.trust_device; // trust device not persisted yet

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
      Ok(see_other("/dashboard"))
    }
    Err(err) => {
      let dest = if is_backup {
        format!(
          "/two-factor?method=backup&error={}",
          encode_query(&portuguese_error_message(&err))
        )
      } else {
        format!(
          "/two-factor?error={}",
          encode_query(&portuguese_error_message(&err))
        )
      };
      Ok(see_other(dest))
    }
  }
}
