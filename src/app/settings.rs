use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  asset::{Asset, asset},
  context::{Cx, app_context},
  icon::{icon, iconify::iconify_icon},
  router::{
    content::{Form, multipart::Multipart},
    error::{SeeOther, see_other},
    page, query_params, route,
  },
  view::{View, component, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::{encode_query, require_user};
use crate::components::avatar::{
  AvatarSize, avatar, avatar_fallback, avatar_image,
};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::card::{
  card, card_content, card_description, card_header, card_title,
};
use crate::components::dialog::{
  dialog, dialog_content, dialog_description, dialog_footer, dialog_header,
  dialog_title,
};
use crate::components::input::input;
use crate::components::separator::separator;
use crate::components::tabs::{tabs, tabs_content, tabs_list, tabs_trigger};
use lindaflor::auth::CREDENTIAL_PROVIDER_ID;
use lindaflor::auth::avatar::{self as auth_avatar, object_store};
use lindaflor::auth::google::GOOGLE_PROVIDER_ID;
use lindaflor::auth::service::{
  self, LinkedAccount, ListedSession, portuguese_error_message,
};
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
  saved: Option<String>,
  error: Option<String>,
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

#[page(GET "/settings")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let su = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<SettingsQuery>(cx)?;
  let tab = query.tab.as_deref().unwrap_or("profile").to_string();
  let saved = query.saved.is_some();
  let error_message = query.error.clone();
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

  Ok(view! {
      <div class="mx-auto max-w-7xl px-4 py-8 md:px-8">
          <h1 class="text-2xl font-bold">"Configurações"</h1>
          <p class="mt-1 mb-8 text-sm text-muted-foreground">
              "Gerencie seu perfil, conta, sessões, segurança, contas vinculadas e exclusão de conta."
          </p>

          tabs(
              tabs_list(
                  tabs_trigger(
                      active: tab == "profile",
                      attrs: attributes! { href="/settings?tab=profile" },
                      "Perfil"
                  )
                  tabs_trigger(
                      active: tab == "account",
                      attrs: attributes! { href="/settings?tab=account" },
                      "Conta"
                  )
                  tabs_trigger(
                      active: tab == "sessions",
                      attrs: attributes! { href="/settings?tab=sessions" },
                      "Sessões"
                  )
                  tabs_trigger(
                      active: tab == "security",
                      attrs: attributes! { href="/settings?tab=security" },
                      "Segurança"
                  )
                  tabs_trigger(
                      active: tab == "linked-accounts",
                      attrs: attributes! { href="/settings?tab=linked-accounts" },
                      "Contas vinculadas"
                  )
                  tabs_trigger(
                      active: tab == "danger",
                      attrs: attributes! { href="/settings?tab=danger" },
                      "Zona de perigo"
                  )
              )
              tabs_content(
                  if saved {
                      <div class="mt-6 rounded-lg border border-border bg-background px-4 py-3 text-sm text-foreground shadow-sm">
                          "Alterações salvas com sucesso."
                      </div>
                  }
                  if let Some(ref msg) = error_message {
                      <div class="mt-6 rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
                          (msg.as_str())
                      </div>
                  }

                  if tab == "profile" {
                      profile_tab(
                          name: user_name.clone(),
                          email: user_email.clone(),
                          initials: user_initials.clone(),
                          avatar_url: avatar_url,
                          has_avatar: has_avatar
                      )
                  } else if tab == "account" {
                      account_tab(email: user_email.clone())
                  } else if tab == "sessions" {
                      sessions_tab(sessions: sessions.clone())
                  } else if tab == "security" {
                      security_tab(
                          two_factor_enabled: two_factor_enabled,
                          pending: pending_setup.clone()
                      )
                  } else if tab == "linked-accounts" {
                      linked_accounts_tab(
                          accounts: accounts.clone(),
                          credential_email: credential_email.clone(),
                          has_google: has_google
                      )
                  } else {
                      danger_tab()
                  }
              )
          )
          <script src=(AVATAR_CROPPER_SCRIPT) defer=""></script>
      </div>
  })
}

#[component]
async fn profile_tab(
  name: String,
  email: String,
  initials: String,
  avatar_url: Option<String>,
  has_avatar: bool,
) -> Result<impl View> {
  let name_value = name.clone();
  Ok(view! {
      card(
          attrs: attributes! { class="mt-6" },
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
                          <form method="post" action="/settings?tab=profile">
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
              <form method="post" action="/settings?tab=profile" class="mt-6 flex flex-col gap-4">
                  <input type="hidden" name="action" value="update_profile">
                  <div class="space-y-2">
                      <label for="name">"Nome de exibição"</label>
                      input(attrs: attributes! { type="text" name="name" id="name" value=(name_value) })
                  </div>
                  button(
                      variant: ButtonVariant::Primary,
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
async fn account_tab(email: String) -> Result<impl View> {
  Ok(view! {
      card(
          attrs: attributes! { class="mt-6" },
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
              <form method="post" action="/settings?tab=account" class="flex flex-col gap-4">
                  <input type="hidden" name="action" value="change_email">
                  <h3 class="text-sm font-medium">"Alterar e-mail"</h3>
                  <div class="space-y-2">
                      <label for="new_email">"Novo e-mail"</label>
                      input(attrs: attributes! { type="email" name="new_email" id="new_email" })
                  </div>
                  button(
                      variant: ButtonVariant::Primary,
                      attrs: attributes! { type="submit" class="w-fit" },
                      "Atualizar e-mail"
                  )
              </form>
              separator(attrs: attributes! { class="my-5" })
              <form method="post" action="/settings?tab=account" class="flex flex-col gap-4">
                  <input type="hidden" name="action" value="change_password">
                  <h3 class="text-sm font-medium">"Alterar senha"</h3>
                  <div class="space-y-2">
                      <label for="current_password">"Senha atual"</label>
                      input(attrs: attributes! { type="password" name="current_password" id="current_password" autocomplete="current-password" })
                  </div>
                  <div class="space-y-2">
                      <label for="new_password">"Nova senha"</label>
                      input(attrs: attributes! { type="password" name="new_password" id="new_password" autocomplete="new-password" })
                  </div>
                  <div class="space-y-2">
                      <label for="confirm_password">"Confirmar nova senha"</label>
                      input(attrs: attributes! { type="password" name="confirm_password" id="confirm_password" autocomplete="new-password" })
                  </div>
                  button(
                      variant: ButtonVariant::Primary,
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
          attrs: attributes! { class="mt-6" },
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
                              <form method="post" action="/settings?tab=sessions">
                                  <input type="hidden" name="action" value="revoke_session">
                                  <input type="hidden" name="session_id" value=(session.id.to_string())>
                                  button(
                                      variant: ButtonVariant::Outline,
                                      size: ButtonSize::Sm,
                                      attrs: attributes! { type="submit" },
                                      "Revogar"
                                  )
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
  two_factor_enabled: bool,
  pending: Option<(String, String)>,
) -> Result<impl View> {
  Ok(view! {
      card(
          attrs: attributes! { class="mt-6" },
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
                      <p class="break-all font-mono text-xs">(secret.as_str())</p>
                      <p class="break-all text-xs text-muted-foreground">(totp_uri.as_str())</p>
                      <form method="post" action="/settings?tab=security" class="flex flex-col gap-4">
                          <input type="hidden" name="action" value="confirm_2fa">
                          <div class="space-y-2">
                              <label for="code">"Código de 6 dígitos"</label>
                              input(attrs: attributes! { type="text" name="code" id="code" inputmode="numeric" maxlength="6" autocomplete="one-time-code" })
                          </div>
                          button(
                              variant: ButtonVariant::Primary,
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
                          <form method="post" action="/settings?tab=security" class="flex items-end gap-2">
                              <input type="hidden" name="action" value="disable_2fa">
                              <div class="space-y-1">
                                  <label for="password" class="text-xs">"Senha"</label>
                                  input(attrs: attributes! { type="password" name="password" id="password" required="" })
                              </div>
                              button(
                                  variant: ButtonVariant::Outline,
                                  size: ButtonSize::Sm,
                                  attrs: attributes! { type="submit" },
                                  "Desativar 2FA"
                              )
                          </form>
                      } else {
                          <form method="post" action="/settings?tab=security">
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
          attrs: attributes! { class="mt-6" },
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
                              <form method="post" action="/settings?tab=linked-accounts">
                                  <input type="hidden" name="action" value="unlink">
                                  <input type="hidden" name="provider_id" value=(account.provider_id.clone())>
                                  <input type="hidden" name="account_id" value=(account.account_id.clone())>
                                  button(
                                      variant: ButtonVariant::Outline,
                                      size: ButtonSize::Sm,
                                      attrs: attributes! { type="submit" },
                                      "Desconectar"
                                  )
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
                          <form method="post" action="/settings?tab=linked-accounts">
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
async fn danger_tab() -> Result<impl View> {
  Ok(view! {
      card(
          attrs: attributes! { class="mt-6" },
          card_header(
              card_title("Zona de perigo")
              card_description(
                  "Exclua permanentemente sua conta e todos os dados associados. Isso não pode ser desfeito."
              )
          )
          card_content(
              <form method="post" action="/settings?tab=danger" class="flex flex-col gap-4">
                  <input type="hidden" name="action" value="delete_user">
                  <div class="space-y-2">
                      <label for="confirm_email">"Seu e-mail"</label>
                      input(attrs: attributes! { type="email" name="confirm_email" id="confirm_email" autocomplete="off" required="" })
                  </div>
                  button(
                      variant: ButtonVariant::Destructive,
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
) -> Result<SeeOther> {
  let su = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);

  let mut file: Option<(String, Vec<u8>)> = None;
  while let Some(field) = multipart.next_field().await? {
    if field.name() != Some("file") {
      continue;
    }
    let content_type = field.content_type().unwrap_or("").to_owned();
    let bytes = field.bytes().await?.to_vec();
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
    Ok(_) => Ok(see_other("/settings?tab=profile&saved=1")),
    Err(err) => Ok(see_other(format!(
      "/settings?tab=profile&error={}",
      encode_query(&err.to_string())
    ))),
  }
}

#[route(POST "/settings")]
pub async fn settings_post(
  cx: &Cx,
  Form(body): Form<SettingsInput>,
) -> Result<SeeOther> {
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
    see_other(format!(
      "/settings?tab={}&error={}",
      encode_query(&tab),
      encode_query(msg)
    ))
  };
  let ok_redirect =
    || see_other(format!("/settings?tab={}&saved=1", encode_query(&tab)));

  match action {
    "remove_avatar" => {
      let store = object_store(cx);
      match auth_avatar::remove_avatar(pool, &store, su.user.id).await {
        Ok(()) => Ok(ok_redirect()),
        Err(err) => Ok(err_redirect(&err.to_string())),
      }
    }
    "update_profile" => {
      let name = body.name.as_deref().unwrap_or("");
      match service::update_user_name(pool, su.user.id, name).await {
        Ok(_) => Ok(ok_redirect()),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
      }
    }
    "change_email" => {
      let new_email = body.new_email.as_deref().unwrap_or("");
      match service::request_change_email(
        pool,
        su.user.id,
        &su.user.email,
        new_email,
        Some("/settings?tab=account"),
      )
      .await
      {
        Ok(()) => Ok(ok_redirect()),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
      }
    }
    "change_password" => {
      let current = body.current_password.as_deref().unwrap_or("");
      let new_password = body.new_password.as_deref().unwrap_or("");
      let confirm = body.confirm_password.as_deref().unwrap_or("");
      if new_password != confirm {
        return Ok(err_redirect("As senhas não conferem."));
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
        Ok(()) => Ok(ok_redirect()),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
      }
    }
    "revoke_session" => {
      let Some(id) = body
        .session_id
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok())
      else {
        return Ok(err_redirect("Sessão inválida."));
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
        Ok(()) => Ok(ok_redirect()),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
      }
    }
    "enable_2fa" => match service::enable_two_factor(pool, &su.user).await {
      Ok(_) => Ok(see_other("/settings?tab=security&setup=1")),
      Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
    },
    "confirm_2fa" => {
      let code = body.code.as_deref().unwrap_or("");
      match service::confirm_enable_two_factor(pool, &su.user, code).await {
        Ok(()) => Ok(ok_redirect()),
        Err(err) => Ok(see_other(format!(
          "/settings?tab=security&setup=1&error={}",
          encode_query(&portuguese_error_message(&err))
        ))),
      }
    }
    "disable_2fa" => {
      let password = body.password.as_deref();
      match service::disable_two_factor(pool, &su.user, password, None).await {
        Ok(()) => Ok(ok_redirect()),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
      }
    }
    "link_google" => {
      match service::start_link_google(
        pool,
        su.user.id,
        Some("/settings?tab=linked-accounts"),
      )
      .await
      {
        Ok(url) => Ok(see_other(url)),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
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
        Ok(()) => Ok(ok_redirect()),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
      }
    }
    "delete_user" => {
      let confirm = body.confirm_email.as_deref().unwrap_or("");
      match service::delete_user_confirmed(cx, pool, &su.user, confirm).await {
        Ok(()) => Ok(see_other("/")),
        Err(err) => Ok(err_redirect(&portuguese_error_message(&err))),
      }
    }
    _ => Ok(see_other(format!("/settings?tab={}", encode_query(&tab)))),
  }
}
