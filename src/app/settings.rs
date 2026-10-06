use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  asset::{Asset, asset},
  context::{Cx, app_context},
  icon::{icon, iconify::iconify_icon},
  router::{
    content::{Form, multipart::Multipart},
    error::bad_request,
    href, page, query_params, response::Response, route,
  },
  runtime::{Event, expr, procedure, signal},
  view::{View, component, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_user;
use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::avatar::{self as auth_avatar, object_store};
use crate::auth::google::GOOGLE_PROVIDER_ID;
use crate::auth::service::{
  self, LinkedAccount, ListedSession, portuguese_error_message,
};
use crate::components::avatar::{
  AvatarSize, avatar, avatar_fallback, avatar_image,
};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::card::{
  card, card_content, card_description, card_header, card_title,
};
use crate::components::container::container;
use crate::components::dialog::{
  dialog, dialog_content, dialog_description, dialog_footer, dialog_header,
  dialog_title,
};
use crate::components::field::{field, field_error, field_label};
use crate::components::input::input;
use crate::components::separator::separator;
use crate::components::tabs::{tabs, tabs_content, tabs_list, tabs_trigger};
use crate::components::toast::{Toast, set_toast, toast_redirect};
use topcoat::view::attributes;

/// Content-hashed URL for the profile-tab cropper. Only the settings page renders it.
const AVATAR_CROPPER_SCRIPT: Asset = asset!("assets/avatar-cropper.js");

#[derive(Deserialize)]
pub struct SettingsInput {
  action: Option<String>,
  name: Option<String>,
  new_email: Option<String>,
  current_password: Option<String>,
  new_password: Option<String>,
  confirm_password: Option<String>,
  session_id: Option<String>,
  provider_id: Option<String>,
  account_id: Option<String>,
  confirm_email: Option<String>,
  code: Option<String>,
  password: Option<String>,
}

#[query_params(error = bad_request)]
struct SettingsQuery {
  tab: Option<String>,
  setup: Option<String>,
}

fn initials(name: &str) -> String {
  name
    .split_whitespace()
    .filter_map(|w| w.chars().next())
    .take(2)
    .collect::<String>()
    .to_uppercase()
}

fn provider_label(provider_id: &str) -> &'static str {
  match provider_id {
    CREDENTIAL_PROVIDER_ID => "E-mail e senha",
    "google" => "Google",
    _ => "Outro",
  }
}

fn normalize_tab(raw: &str) -> String {
  match raw.trim() {
    "account" | "sessions" | "security" | "linked-accounts" | "danger" => {
      raw.trim().to_string()
    }
    _ => "profile".to_string(),
  }
}

/// Build a `data:image/svg+xml` URI for a TOTP `otpauth://` URI so the
/// settings page can show a scannable QR code with a plain `<img>`.
/// Returns `None` when the payload cannot be encoded as a QR code.
fn totp_qr_image_uri(totp_uri: &str) -> Option<String> {
  use qrcode::{QrCode, render::svg};
  let code = QrCode::new(totp_uri).ok()?;
  let svg_xml: String =
    code.render::<svg::Color>().min_dimensions(192, 192).build();
  let mut uri = String::from("data:image/svg+xml,");
  // Percent-encode characters that cannot appear in a double-quoted
  // HTML attribute / CSS url(), mirroring `checkmark_style` in
  // `components/select.rs` plus `<`/`>` for `src` attributes.
  for c in svg_xml.chars() {
    match c {
      '%' => uri.push_str("%25"),
      '"' => uri.push_str("%22"),
      '#' => uri.push_str("%23"),
      '<' => uri.push_str("%3C"),
      '>' => uri.push_str("%3E"),
      _ => uri.push(c),
    }
  }
  Some(uri)
}

#[page(GET "/settings")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let su = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<SettingsQuery>(cx)?;
  let tab = query.tab.as_deref().unwrap_or("profile").to_string();
  let setup_mode = query.setup.as_deref() == Some("1");

  let sessions = service::list_user_sessions(cx, pool, su.user.id).await?;
  let accounts = service::list_linked_accounts(pool, su.user.id).await?;
  let pending_setup = if setup_mode || !su.user.two_factor_enabled {
    service::pending_two_factor_setup(pool, su.user.id).await?
  } else {
    None
  };

  let user_name = su.user.name.clone();
  let user_email = su.user.email.clone();
  let user_initials = initials(&user_name);
  let store = object_store(cx);
  let avatar_url =
    auth_avatar::resolve_avatar_url(&store, su.user.image.as_deref()).await;
  let has_avatar = su
    .user
    .image
    .as_deref()
    .is_some_and(|image| !image.is_empty());
  let two_factor_enabled = su.user.two_factor_enabled;
  let has_google = accounts.iter().any(|a| a.provider_id == GOOGLE_PROVIDER_ID);
  let credential_email = accounts
    .iter()
    .find(|a| a.provider_id == CREDENTIAL_PROVIDER_ID)
    .map(|_| user_email.clone());

  // Tab selection as a signal: switching updates `active`/`hidden` bindings
  // in the browser without a document reload. `href` remains as a no-JS
  // fallback (`?tab=` is still honored on first render).
  let active = signal(cx, || normalize_tab(&tab));

  Ok(view! {
      container(
          <h1 class="text-2xl font-bold">"Configurações"</h1>
          <p class="text-sm text-muted-foreground">
              "Gerencie seu perfil, conta, sessões, segurança, contas vinculadas e exclusão de conta."
          </p>

          tabs(
            tabs_list(
                  tabs_trigger(
                      active: $(active.get() == "profile"),
                      attrs: attributes! {
                          href=(href!(page).query([("tab", "profile")]))
                          @click=$(|e: Event| { e.prevent_default(); active.set("profile".to_owned()); })
                      },
                      "Perfil"
                  )
                  tabs_trigger(
                      active: $(active.get() == "account"),
                      attrs: attributes! {
                          href=(href!(page).query([("tab", "account")]))
                          @click=$(|e: Event| { e.prevent_default(); active.set("account".to_owned()); })
                      },
                      "Conta"
                  )
                  tabs_trigger(
                      active: $(active.get() == "sessions"),
                      attrs: attributes! {
                          href=(href!(page).query([("tab", "sessions")]))
                          @click=$(|e: Event| { e.prevent_default(); active.set("sessions".to_owned()); })
                      },
                      "Sessões"
                  )
                  tabs_trigger(
                      active: $(active.get() == "security"),
                      attrs: attributes! {
                          href=(href!(page).query([("tab", "security")]))
                          @click=$(|e: Event| { e.prevent_default(); active.set("security".to_owned()); })
                      },
                      "Segurança"
                  )
                  tabs_trigger(
                      active: $(active.get() == "linked-accounts"),
                      attrs: attributes! {
                          href=(href!(page).query([("tab", "linked-accounts")]))
                          @click=$(|e: Event| { e.prevent_default(); active.set("linked-accounts".to_owned()); })
                      },
                      "Contas vinculadas"
                  )
                  tabs_trigger(
                      active: $(active.get() == "danger"),
                      attrs: attributes! {
                          href=(href!(page).query([("tab", "danger")]))
                          @click=$(|e: Event| { e.prevent_default(); active.set("danger".to_owned()); })
                      },
                      "Zona de perigo"
                  )
              )
              tabs_content(
                  <div :hidden=$(active.get() != "profile")>
                      profile_tab(
                          name: user_name.clone(),
                          email: user_email.clone(),
                          initials: user_initials.clone(),
                          avatar_url: avatar_url,
                          has_avatar: has_avatar
                      )
                  </div>
                  <div :hidden=$(active.get() != "account")>
                      account_tab(email: user_email.clone())
                  </div>
                  <div :hidden=$(active.get() != "sessions")>
                      sessions_tab(sessions: sessions.clone())
                  </div>
                  <div :hidden=$(active.get() != "security")>
                      security_tab(
                          two_factor_enabled: two_factor_enabled,
                          pending: pending_setup.clone()
                      )
                  </div>
                  <div :hidden=$(active.get() != "linked-accounts")>
                      linked_accounts_tab(
                          accounts: accounts.clone(),
                          credential_email: credential_email.clone(),
                          has_google: has_google
                      )
                  </div>
                  <div :hidden=$(active.get() != "danger")>
                      danger_tab()
                  </div>
              )
          )
          <script src=(AVATAR_CROPPER_SCRIPT) defer=""></script>
      )
  })
}

#[component]
async fn profile_tab(
  cx: &Cx,
  name: String,
  email: String,
  initials: String,
  avatar_url: Option<String>,
  has_avatar: bool,
) -> Result<impl View> {
  let name_value = name.clone();
  let display_name = signal(cx, || name_value.clone());
  let display_name_touched = signal(cx, || false);
  let display_name_error = expr!({
    if !display_name_touched.get() {
      "".to_owned()
    } else if display_name.get().trim().is_empty() {
      "Informe o nome.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let profile_blocked = expr!({ display_name.get().trim().is_empty() });
  Ok(view! {
      card(
          card_header(
              card_title("Perfil")
              card_description("Seu nome e avatar.")
          )
          card_content(
              <div class="flex items-center gap-4">
                  avatar(
                      size: AvatarSize::Lg,
                      if let Some(url) = avatar_url {
                          avatar_image(attrs: attributes! { src=(url) })
                      }
                      avatar_fallback((initials))
                  )
                  <div>
                      <p class="font-semibold">(name.as_str())</p>
                      <p class="text-sm text-muted-foreground">(email.as_str())</p>
                      <div class="flex items-center gap-2 mt-1">
                      button(
                          variant: ButtonVariant::Outline,
                          size: ButtonSize::Sm,
                          attrs: attributes! { type="button" id="avatar-cropper-open" },
                          "Alterar foto"
                      )
                      if has_avatar {
                          <form
                              method="post"
                              action=(href!(settings_post).query([("tab", "profile")]))
                              data-toast-promise=""
                              data-toast-loading="Removendo foto…"
                          >
                              <input type="hidden" name="action" value="remove_avatar">
                              button(
                                  variant: ButtonVariant::Outline,
                                  size: ButtonSize::Sm,
                                  attrs: attributes! { type="submit" class="w-fit" },
                                  "Remover"
                              )
                          </form>
                      }
                      </div>
                  </div>
              </div>
              <form
                  method="post"
                  action=(href!(settings_post).query([("tab", "profile")]))
                  class="mt-6 flex flex-col gap-4"
                  novalidate=""
                  data-toast-promise=""
                  data-toast-loading="Salvando…"
              >
                  <input type="hidden" name="action" value="update_profile">
                  field(
                      attrs: attributes! {
                          :data-invalid=$( (!display_name_error.is_empty()).then_some("true") )
                      },
                      field_label(attrs: attributes! { for="name" }, "Nome de exibição")
                      input(
                          value: display_name,
                          touched: display_name_touched.clone(),
                          error: display_name_error.clone(),
                          attrs: attributes! {
                              id="name"
                              name="name"
                              type="text"
                              autocomplete="name"
                              aria-describedby="name-error"
                          }
                      )
                      field_error(
                          message: display_name_error,
                          attrs: attributes! { id="name-error" }
                      )
                  )
                  button(
                      blocked: profile_blocked,
                      attrs: attributes! { type="submit" class="w-fit" },
                      "Salvar alterações"
                  )
              </form>
              dialog(
                  open: false,
                  attrs: attributes! {
                      id="avatar-cropper-dialog"
                      data-avatar-cropper=""
                      aria-labelledby="avatar-cropper-title"
                  },
                  dialog_content(
                      dialog_header(
                          dialog_title(
                              attrs: attributes! { id="avatar-cropper-title" },
                              "Alterar avatar"
                          )
                          dialog_description(
                              "Selecione uma imagem e ajuste o recorte circular."
                          )
                      )
                      <p id="avatar-cropper-error" class="hidden text-sm text-destructive" role="alert"></p>
                      <button
                          type="button"
                          id="avatar-cropper-drop"
                          class="mx-auto flex size-64 cursor-pointer flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border bg-foreground/5 px-4 text-center text-sm text-foreground hover:bg-foreground/10"
                      >
                          "Clique ou arraste uma imagem"
                      </button>
                      <div
                          id="avatar-cropper-stage"
                          class="relative mx-auto hidden size-64 touch-none cursor-grab overflow-hidden rounded-lg bg-foreground/5 select-none"
                      >
                          <img
                              id="avatar-cropper-image"
                              alt=""
                              draggable="false"
                              class="pointer-events-none absolute top-0 left-0 max-w-none select-none"
                          >
                          <div class="pointer-events-none absolute inset-0 rounded-full shadow-[0_0_0_999px_#00000080] ring-2 ring-white/90"></div>
                      </div>
                      <p class="text-center text-xs text-muted-foreground">"JPG, PNG ou WebP · 2 MB"</p>
                      <div id="avatar-cropper-controls" class="hidden items-center gap-2">
                          button(
                              variant: ButtonVariant::Outline,
                              size: ButtonSize::Icon,
                              attrs: attributes! {
                                  type="button"
                                  id="avatar-cropper-zoom-out"
                                  aria-label="Diminuir zoom"
                              },
                              icon(data: iconify_icon!("lucide:minus"))
                          )
                          <input
                              id="avatar-cropper-zoom"
                              type="range"
                              min="1"
                              max="5"
                              step="0.1"
                              value="1"
                              aria-label="Zoom"
                              class="h-2 min-w-0 flex-1 cursor-pointer accent-primary"
                          >
                          button(
                              variant: ButtonVariant::Outline,
                              size: ButtonSize::Icon,
                              attrs: attributes! {
                                  type="button"
                                  id="avatar-cropper-zoom-in"
                                  aria-label="Aumentar zoom"
                              },
                              icon(data: iconify_icon!("lucide:plus"))
                          )
                          button(
                              variant: ButtonVariant::Outline,
                              size: ButtonSize::Icon,
                              attrs: attributes! {
                                  type="button"
                                  id="avatar-cropper-reset"
                                  aria-label="Redefinir recorte"
                              },
                              icon(data: iconify_icon!("lucide:rotate-ccw"))
                          )
                      </div>
                      <input
                          id="avatar-cropper-file"
                          type="file"
                          accept="image/jpeg,image/png,image/webp"
                          class="sr-only"
                      >
                      dialog_footer(
                          button(
                              variant: ButtonVariant::Outline,
                              size: ButtonSize::Sm,
                              attrs: attributes! { type="button" id="avatar-cropper-choose" },
                              "Escolher outra imagem"
                          )
                          button(
                              variant: ButtonVariant::Primary,
                              size: ButtonSize::Sm,
                              attrs: attributes! { type="button" id="avatar-cropper-apply" disabled="" },
                              "Aplicar"
                          )
                      )
                  )
              )
          )
      )
  })
}

#[component]
async fn account_tab(cx: &Cx, email: String) -> Result<impl View> {
  let min_password_len = crate::auth::service::MIN_PASSWORD_LEN as f64;
  let new_email = signal(cx, String::new);
  let new_email_touched = signal(cx, || false);
  let new_email_error = expr!({
    if !new_email_touched.get() {
      "".to_owned()
    } else if new_email.get().trim().is_empty() {
      "Informe o e-mail.".to_owned()
    } else if !new_email.get().contains("@") {
      "E-mail inválido.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let current_password = signal(cx, String::new);
  let current_password_touched = signal(cx, || false);
  let current_password_error = expr!({
    if !current_password_touched.get() {
      "".to_owned()
    } else if current_password.get().is_empty() {
      "Informe a senha atual.".to_owned()
    } else {
      "".to_owned()
    }
  });

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
  let email_blocked = expr!({
    if new_email.get().trim().is_empty() {
      true
    } else if !new_email.get().contains("@") {
      true
    } else {
      false
    }
  });
  let password_blocked = expr!({
    if current_password.get().is_empty() {
      true
    } else if new_password.get().is_empty() {
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
      card(
          card_header(
              card_title("Conta")
              card_description(
                  <span>
                      "E-mail atual: "
                      <strong>(email.as_str())</strong>
                  </span>
              )
          )
          card_content(
              <form
                  
                      method="post"
                      action=(href!(settings_post).query([("tab", "account")]))
                      class="flex flex-col gap-4"
                    novalidate=""
                    data-toast-promise=""
                    data-toast-loading="Atualizando e-mail…"
                >
                  <input type="hidden" name="action" value="change_email">
                  <h3 class="text-sm font-medium">"Alterar e-mail"</h3>
                  field(
                      attrs: attributes! {
                          :data-invalid=$( (!new_email_error.is_empty()).then_some("true") )
                      },
                      field_label(attrs: attributes! { for="new_email" }, "Novo e-mail")
                      input(
                          value: new_email,
                          touched: new_email_touched.clone(),
                          error: new_email_error.clone(),
                          attrs: attributes! {
                              id="new_email"
                              name="new_email"
                              type="email"
                              autocomplete="email"
                              aria-describedby="new_email-error"
                          }
                      )
                      field_error(
                          message: new_email_error,
                          attrs: attributes! { id="new_email-error" }
                      )
                  )
                  button(
                      blocked: email_blocked,
                      attrs: attributes! { type="submit" class="w-fit" },
                      "Atualizar e-mail"
                  )
              </form>
              separator(attrs: attributes! { class="my-5" })
              <form
                  
                    method="post"
                    action=(href!(settings_post).query([("tab", "account")]))
                    class="flex flex-col gap-4"
                    novalidate=""
                    data-toast-promise=""
                    data-toast-loading="Atualizando senha…"
                >
                  <input type="hidden" name="action" value="change_password">
                  <h3 class="text-sm font-medium">"Alterar senha"</h3>
                  field(
                      attrs: attributes! {
                          :data-invalid=$( (!current_password_error.is_empty()).then_some("true") )
                      },
                      field_label(attrs: attributes! { for="current_password" }, "Senha atual")
                      input(
                          value: current_password,
                          touched: current_password_touched.clone(),
                          error: current_password_error.clone(),
                          attrs: attributes! {
                              id="current_password"
                              name="current_password"
                              type="password"
                              autocomplete="current-password"
                              aria-describedby="current_password-error"
                          }
                      )
                      field_error(
                          message: current_password_error,
                          attrs: attributes! { id="current_password-error" }
                      )
                  )
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
                      blocked: password_blocked,
                      attrs: attributes! { type="submit" class="w-fit" },
                      "Atualizar senha"
                  )
              </form>
          )
      )
  })
}

#[component]
async fn sessions_tab(sessions: Vec<ListedSession>) -> Result<impl View> {
  Ok(view! {
      card(
          card_header(
              card_title("Sessões ativas")
              card_description("Dispositivos atualmente conectados à sua conta.")
          )
          card_content(
              <div class="flex flex-col gap-3">
                  for session in sessions {
                      <div class="flex items-center justify-between rounded-lg border border-border p-4">
                          <div>
                              <p class="text-sm font-medium">
                                  (session.user_agent.as_deref().unwrap_or("Dispositivo desconhecido"))
                                  if session.current {
                                      badge(
                                          variant: BadgeVariant::Secondary,
                                          attrs: attributes! { class="ml-2" },
                                          "Este dispositivo"
                                      )
                                  }
                              </p>
                              <p class="mt-1 text-xs text-muted-foreground">
                                  (session.ip_address.as_deref().unwrap_or("—"))
                              </p>
                          </div>
                          if !session.current {
                              <form
                                  method="post"
                                      action=(href!(settings_post).query([("tab", "sessions")]))
                                      data-toast-promise=""
                                      data-toast-loading="Revogando…"
                                  >
                                  <input type="hidden" name="action" value="revoke_session">
                                  <input type="hidden" name="session_id" value=(session.id.to_string())>
                                  {
                                      let sid = session.id.to_string();
                                      button(
                                          variant: ButtonVariant::Outline,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! {
                                              type="submit"
                                              @click=$(async |e: Event| {
                                                  e.prevent_default();
                                                  revoke_session_proc(sid.to_owned()).await;
                                                  raw!("location.reload()");
                                              })
                                          },
                                          "Revogar"
                                      )
                                  }
                              </form>
                          }
                      </div>
                  }
              </div>
          )
      )
  })
}

#[component]
async fn security_tab(
  cx: &Cx,
  two_factor_enabled: bool,
  pending: Option<(String, String)>,
) -> Result<impl View> {
  let qr_uri = pending.as_ref().and_then(|(_, uri)| totp_qr_image_uri(uri));
  let totp_code = signal(cx, String::new);
  let totp_code_touched = signal(cx, || false);
  let totp_code_error = expr!({
    if !totp_code_touched.get() {
      "".to_owned()
    } else if totp_code.get().trim().is_empty() {
      "Informe o código de 6 dígitos.".to_owned()
    } else if totp_code.get().trim().len() != 6.0 {
      "O código deve ter 6 dígitos.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let disable_password = signal(cx, String::new);
  let disable_password_touched = signal(cx, || false);
  let disable_password_error = expr!({
    if !disable_password_touched.get() {
      "".to_owned()
    } else if disable_password.get().is_empty() {
      "Informe a senha.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let totp_blocked = expr!({
    if totp_code.get().trim().is_empty() {
      true
    } else if totp_code.get().trim().len() != 6.0 {
      true
    } else {
      false
    }
  });
  let disable_blocked = expr!({ disable_password.get().is_empty() });
  Ok(view! {
      card(
          card_header(
              card_title("Segurança")
              card_description(
                  "Adicione uma etapa extra ao login usando um aplicativo autenticador."
              )
          )
          card_content(
              if let Some((secret, totp_uri)) = pending {
                  <div class="flex flex-col gap-4">
                      <p class="text-sm text-muted-foreground">
                          "Escaneie o código no app autenticador ou digite o segredo manualmente, depois confirme com um código de 6 dígitos."
                      </p>
                      if let Some(uri) = qr_uri {
                          <div class="flex flex-col items-center gap-2">
                              <div class="rounded-lg border border-border bg-white p-3">
                                  <img src=(uri) alt="QR code para aplicativo autenticador" width="192" height="192" class="size-48">
                              </div>
                          </div>
                      }
                      <p class="break-all font-mono text-xs">(secret.as_str())</p>
                      <details class="text-xs text-muted-foreground">
                          <summary class="cursor-pointer">"Detalhes para configuração manual"</summary>
                          <p class="mt-1 break-all">(totp_uri.as_str())</p>
                      </details>
                      <form
                              method="post"
                              action=(href!(settings_post).query([("tab", "security")]))
                              class="flex flex-col gap-4"
                              novalidate=""
                              data-toast-promise=""
                              data-toast-loading="Confirmando…"
                          >
                          <input type="hidden" name="action" value="confirm_2fa">
                          field(
                              attrs: attributes! {
                                  :data-invalid=$( (!totp_code_error.is_empty()).then_some("true") )
                              },
                              field_label(attrs: attributes! { for="code" }, "Código de 6 dígitos")
                              input(
                                  value: totp_code,
                                  touched: totp_code_touched.clone(),
                                  error: totp_code_error.clone(),
                                  attrs: attributes! {
                                      id="code"
                                      name="code"
                                      type="text"
                                      inputmode="numeric"
                                      maxlength="6"
                                      autocomplete="one-time-code"
                                      aria-describedby="code-error"
                                  }
                              )
                              field_error(
                                  message: totp_code_error,
                                  attrs: attributes! { id="code-error" }
                              )
                          )
                          button(
                              blocked: totp_blocked,
                              attrs: attributes! { type="submit" class="w-fit" },
                              "Confirmar 2FA"
                          )
                      </form>
                  </div>
              } else {
                  <div class="flex items-center justify-between rounded-lg border border-border p-4">
                      <div>
                          <p class="text-sm font-medium">"Autenticação de dois fatores"</p>
                          <p class="mt-1 text-xs text-muted-foreground">
                              if two_factor_enabled {
                                  "A 2FA está ativada."
                              } else {
                                  "A 2FA está desativada."
                              }
                          </p>
                      </div>
                      if two_factor_enabled {
                          <form
                              method="post"
                                  action=(href!(settings_post).query([("tab", "security")]))
                                  class="flex items-end gap-2"
                                  novalidate=""
                                  data-toast-promise=""
                                  data-toast-loading="Desativando…"
                              >
                              <input type="hidden" name="action" value="disable_2fa">
                              field(
                                  attrs: attributes! {
                                      :data-invalid=$( (!disable_password_error.is_empty()).then_some("true") )
                                  },
                                  field_label(attrs: attributes! { for="password" }, "Senha")
                                  input(
                                      value: disable_password,
                                      touched: disable_password_touched.clone(),
                                      error: disable_password_error.clone(),
                                      attrs: attributes! {
                                          id="password"
                                          name="password"
                                          type="password"
                                          autocomplete="current-password"
                                          aria-describedby="password-error"
                                      }
                                  )
                                  field_error(
                                      message: disable_password_error,
                                      attrs: attributes! { id="password-error" }
                                  )
                              )
                              button(
                                  variant: ButtonVariant::Outline,
                                  size: ButtonSize::Sm,
                                  blocked: disable_blocked,
                                  attrs: attributes! { type="submit" },
                                  "Desativar 2FA"
                              )
                          </form>
                      } else {
                          <form
                              method="post"
                                  action=(href!(settings_post).query([("tab", "security")]))
                                  data-toast-promise=""
                                  data-toast-loading="Ativando…"
                              >
                              <input type="hidden" name="action" value="enable_2fa">
                              button(
                                  variant: ButtonVariant::Primary,
                                  size: ButtonSize::Sm,
                                  attrs: attributes! { type="submit" },
                                  "Ativar 2FA"
                              )
                          </form>
                      }
                  </div>
              }
          )
      )
  })
}

#[component]
async fn linked_accounts_tab(
  accounts: Vec<LinkedAccount>,
  credential_email: Option<String>,
  has_google: bool,
) -> Result<impl View> {
  Ok(view! {
      card(
          card_header(
              card_title("Contas vinculadas")
              card_description("Métodos de login vinculados à sua conta.")
          )
          card_content(
              <div class="flex flex-col gap-3">
                  for account in accounts {
                      <div class="flex items-center justify-between rounded-lg border border-border p-4">
                          <div>
                              <p class="text-sm font-medium">(provider_label(&account.provider_id))</p>
                              <p class="mt-1 text-xs text-muted-foreground">
                                  if account.provider_id == CREDENTIAL_PROVIDER_ID {
                                      (credential_email.as_deref().unwrap_or(&account.account_id))
                                  } else {
                                      (account.account_id.as_str())
                                  }
                              </p>
                          </div>
                          if account.provider_id != CREDENTIAL_PROVIDER_ID {
                              <form
                                  method="post"
                                      action=(href!(settings_post).query([("tab", "linked-accounts")]))
                                      data-toast-promise=""
                                      data-toast-loading="Desconectando…"
                                  >
                                  <input type="hidden" name="action" value="unlink">
                                  <input type="hidden" name="provider_id" value=(account.provider_id.clone())>
                                  <input type="hidden" name="account_id" value=(account.account_id.clone())>
                                  {
                                      let pid = account.provider_id.clone();
                                      let aid = account.account_id.clone();
                                      button(
                                          variant: ButtonVariant::Outline,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! {
                                              type="submit"
                                              @click=$(async |e: Event| {
                                                  e.prevent_default();
                                                  unlink_account_proc(pid.to_owned(), aid.to_owned()).await;
                                                  raw!("location.reload()");
                                              })
                                          },
                                          "Desconectar"
                                      )
                                  }
                              </form>
                          }
                      </div>
                  }
                  if !has_google {
                      <div class="flex items-center justify-between rounded-lg border border-border p-4">
                          <div>
                              <p class="text-sm font-medium">"Google"</p>
                              <p class="mt-1 text-xs text-muted-foreground">
                                  "Não conectado"
                              </p>
                          </div>
                          <form
                              method="post"
                                  action=(href!(settings_post).query([("tab", "linked-accounts")]))
                                  data-toast-promise=""
                                  data-toast-loading="Conectando…"
                              >
                              <input type="hidden" name="action" value="link_google">
                              button(
                                  variant: ButtonVariant::Outline,
                                  size: ButtonSize::Sm,
                                  attrs: attributes! { type="submit" },
                                  "Conectar"
                              )
                          </form>
                      </div>
                  }
              </div>
          )
      )
  })
}

#[component]
async fn danger_tab(cx: &Cx) -> Result<impl View> {
  let confirm_email = signal(cx, String::new);
  let confirm_email_touched = signal(cx, || false);
  let confirm_email_error = expr!({
    if !confirm_email_touched.get() {
      "".to_owned()
    } else if confirm_email.get().trim().is_empty() {
      "Informe o e-mail.".to_owned()
    } else if !confirm_email.get().contains("@") {
      "E-mail inválido.".to_owned()
    } else {
      "".to_owned()
    }
  });
  let delete_blocked = expr!({
    if confirm_email.get().trim().is_empty() {
      true
    } else if !confirm_email.get().contains("@") {
      true
    } else {
      false
    }
  });
  Ok(view! {
      card(
          card_header(
              card_title("Zona de perigo")
              card_description(
                  "Exclua permanentemente sua conta e todos os dados associados. Isso não pode ser desfeito."
              )
          )
          card_content(
              <form
                  method="post"
                      action=(href!(settings_post).query([("tab", "danger")]))
                      class="flex flex-col gap-4"
                      novalidate=""
                      data-toast-promise=""
                      data-toast-loading="Excluindo conta…"
                  >
                  <input type="hidden" name="action" value="delete_user">
                  field(
                      attrs: attributes! {
                          :data-invalid=$( (!confirm_email_error.is_empty()).then_some("true") )
                      },
                      field_label(attrs: attributes! { for="confirm_email" }, "Seu e-mail")
                      input(
                          value: confirm_email,
                          touched: confirm_email_touched.clone(),
                          error: confirm_email_error.clone(),
                          attrs: attributes! {
                              id="confirm_email"
                              name="confirm_email"
                              type="email"
                              autocomplete="off"
                              aria-describedby="confirm_email-error"
                          }
                      )
                      field_error(
                          message: confirm_email_error,
                          attrs: attributes! { id="confirm_email-error" }
                      )
                  )
                  button(
                      variant: ButtonVariant::Destructive,
                      blocked: delete_blocked,
                      attrs: attributes! { type="submit" class="w-fit" },
                      "Excluir conta"
                  )
              </form>
          )
      )
  })
}

#[route(POST "/settings/avatar")]
pub async fn upload_avatar(
  cx: &Cx,
  mut multipart: Multipart,
) -> Result<Response> {
  let su = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);

  let mut file: Option<(String, Vec<u8>)> = None;
  while let Some(part) = multipart.next_field().await? {
    if part.name() != Some("file") {
      continue;
    }
    let content_type = part.content_type().unwrap_or("").to_owned();
    let bytes = part.bytes().await?.to_vec();
    file = Some((content_type, bytes));
  }

  let (content_type, bytes) = file.unwrap_or_default();
  match auth_avatar::update_avatar(
    pool,
    &store,
    su.user.id,
    &bytes,
    &content_type,
  )
  .await
  {
    Ok(_) => {
      set_toast(cx, Toast::success("Alterações salvas com sucesso."));
      toast_redirect(cx, href!(page).query([("tab", "profile")]).resolve(cx))
    }
    Err(err) => {
      set_toast(cx, Toast::error(err.to_string()));
      toast_redirect(cx, href!(page).query([("tab", "profile")]).resolve(cx))
    }
  }
}

/// JS fast-path for the `revoke_session` form above. The hidden
/// `action`/`session_id` inputs stay as the no-JS fallback; the browser calls
/// this with the session id directly instead of posting the form.
#[procedure("/settings/revoke-session")]
async fn revoke_session_proc(cx: &Cx, session_id: String) -> Result<bool> {
  let su = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let id = Uuid::parse_str(session_id.trim())
    .map_err(|_| bad_request("Sessão inválida"))?;
  service::revoke_user_session(cx, pool, su.user.id, su.session_id, id).await?;
  Ok(true)
}

/// JS fast-path for the `unlink` form above. Hidden `provider_id`/`account_id`
/// inputs stay as the no-JS fallback.
#[procedure("/settings/unlink-account")]
async fn unlink_account_proc(
  cx: &Cx,
  provider_id: String,
  account_id: String,
) -> Result<bool> {
  let su = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  service::unlink_linked_account(pool, su.user.id, &provider_id, &account_id)
    .await?;
  Ok(true)
}

fn settings_tab_url(cx: &Cx, tab: &str) -> String {
  href!(page).query([("tab", tab)]).resolve(cx)
}

#[route(POST "/settings")]
pub async fn settings_post(
  cx: &Cx,
  Form(body): Form<SettingsInput>,
) -> Result<Response> {
  let su = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let tab = topcoat::router::request::uri(cx)
    .query()
    .and_then(|q| {
      q.split('&')
        .find_map(|p| p.strip_prefix("tab=").map(str::to_owned))
    })
    .unwrap_or_else(|| "profile".to_string());

  let action = body.action.as_deref().unwrap_or("");
  let err_redirect = |msg: &str| {
    set_toast(cx, Toast::error(msg));
    toast_redirect(cx, settings_tab_url(cx, &tab))
  };
  let ok_redirect = || {
    set_toast(cx, Toast::success("Alterações salvas com sucesso."));
    toast_redirect(cx, settings_tab_url(cx, &tab))
  };

  match action {
    "remove_avatar" => {
      let store = object_store(cx);
      match auth_avatar::remove_avatar(pool, &store, su.user.id).await {
        Ok(()) => ok_redirect(),
        Err(err) => err_redirect(&err.to_string()),
      }
    }
    "update_profile" => {
      let name = body.name.as_deref().unwrap_or("");
      match service::update_user_name(pool, su.user.id, name).await {
        Ok(_) => ok_redirect(),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    "change_email" => {
      let new_email = body.new_email.as_deref().unwrap_or("");
      let callback = href!(page).query([("tab", "account")]).resolve(cx);
      match service::request_change_email(
        pool,
        su.user.id,
        &su.user.email,
        new_email,
        Some(callback.as_str()),
      )
      .await
      {
        Ok(()) => ok_redirect(),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    "change_password" => {
      let current = body.current_password.as_deref().unwrap_or("");
      let new_password = body.new_password.as_deref().unwrap_or("");
      let confirm = body.confirm_password.as_deref().unwrap_or("");
      if new_password != confirm {
        return err_redirect("As senhas não conferem.");
      }
      match service::change_password(
        pool,
        su.user.id,
        su.session_id,
        current,
        new_password,
        true,
      )
      .await
      {
        Ok(()) => ok_redirect(),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    "revoke_session" => {
      let Some(id) = body
        .session_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
      else {
        return err_redirect("Sessão inválida.");
      };
      match service::revoke_user_session(
        cx,
        pool,
        su.user.id,
        su.session_id,
        id,
      )
      .await
      {
        Ok(()) => ok_redirect(),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    "enable_2fa" => match service::enable_two_factor(pool, &su.user).await {
      Ok(_) => toast_redirect(
        cx,
        href!(page)
          .query([("tab", "security"), ("setup", "1")])
          .resolve(cx),
      ),
      Err(err) => err_redirect(&portuguese_error_message(&err)),
    },
    "confirm_2fa" => {
      let code = body.code.as_deref().unwrap_or("");
      match service::confirm_enable_two_factor(pool, &su.user, code).await {
        Ok(()) => ok_redirect(),
        Err(err) => {
          set_toast(cx, Toast::error(portuguese_error_message(&err)));
          toast_redirect(
            cx,
            href!(page)
              .query([("tab", "security"), ("setup", "1")])
              .resolve(cx),
          )
        }
      }
    }
    "disable_2fa" => {
      let password = body.password.as_deref();
      match service::disable_two_factor(pool, &su.user, password, None).await {
        Ok(()) => ok_redirect(),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    "link_google" => {
      let callback =
        href!(page).query([("tab", "linked-accounts")]).resolve(cx);
      match service::start_link_google(
        pool,
        su.user.id,
        Some(callback.as_str()),
      )
      .await
      {
        Ok(url) => toast_redirect(cx, url),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    "unlink" => {
      let provider_id = body.provider_id.as_deref().unwrap_or("");
      let account_id = body.account_id.as_deref().unwrap_or("");
      match service::unlink_linked_account(
        pool,
        su.user.id,
        provider_id,
        account_id,
      )
      .await
      {
        Ok(()) => ok_redirect(),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    "delete_user" => {
      let confirm = body.confirm_email.as_deref().unwrap_or("");
      match service::delete_user_confirmed(cx, pool, &su.user, confirm).await {
        Ok(()) => toast_redirect(cx, href!(crate::app::page).resolve(cx)),
        Err(err) => err_redirect(&portuguese_error_message(&err)),
      }
    }
    _ => toast_redirect(cx, settings_tab_url(cx, &tab)),
  }
}
