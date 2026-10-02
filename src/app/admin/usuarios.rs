use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  icon::{icon, iconify::iconify_icon},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    page, query_params, route,
  },
  runtime::{Event, shard, signal},
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::{encode_query, require_user};
use crate::components::alert::{AlertVariant, alert, alert_title};
use crate::components::alert_dialog::alert_dialog;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::dialog::{
  dialog, dialog_content, dialog_description, dialog_footer, dialog_header,
  dialog_title,
};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::pagination::{
  pagination, pagination_content, pagination_item,
};
use crate::components::select::select;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use lindaflor::auth::service;
use lindaflor::auth::user::SessionUser;

const ACTION_LINK: &str = "flex w-full items-center rounded-md px-2 py-1.5 text-left text-sm whitespace-nowrap hover:bg-foreground/5";
const ACTION_DANGER: &str = "flex w-full items-center rounded-md px-2 py-1.5 text-left text-sm whitespace-nowrap text-destructive hover:bg-destructive/10";

#[derive(Deserialize)]
struct IdForm {
  user_id: String,
  q: Option<String>,
  page: Option<String>,
}

#[derive(Deserialize)]
struct RoleForm {
  user_id: String,
  role: String,
  q: Option<String>,
  page: Option<String>,
}

#[derive(Deserialize)]
struct NameForm {
  user_id: String,
  name: String,
  q: Option<String>,
  page: Option<String>,
}

#[derive(Deserialize)]
struct BanForm {
  user_id: String,
  ban_reason: Option<String>,
  q: Option<String>,
  page: Option<String>,
}

#[derive(Deserialize)]
struct RevokeForm {
  session_id: String,
  user_id: String,
  q: Option<String>,
  page: Option<String>,
}

#[query_params(error = bad_request)]
struct UsersQuery {
  q: Option<String>,
  page: Option<String>,
  erro: Option<String>,
  papel: Option<String>,
  editar: Option<String>,
  banir: Option<String>,
  desbanir: Option<String>,
  sessoes: Option<String>,
  revogar_todas: Option<String>,
  remover: Option<String>,
}

struct UserRow {
  id: String,
  menu_id: String,
  menu_target: String,
  trigger_style: String,
  panel_style: String,
  q: String,
  page: String,
  name: String,
  email: String,
  role_label: &'static str,
  role_variant: BadgeVariant,
  banned: bool,
  show_ban: bool,
  show_unban: bool,
  show_impersonate: bool,
  show_remove: bool,
  href_papel: String,
  href_editar: String,
  href_banir: String,
  href_desbanir: String,
  href_sessoes: String,
  href_revogar: String,
  href_remover: String,
}

struct SessionView {
  id: String,
  user_id: String,
  q: String,
  page: String,
  short_id: String,
  created_at: String,
  expires_at: String,
  ip_address: String,
  user_agent: String,
}

enum PanelKind {
  Role,
  Edit,
  Ban,
  Unban,
  Sessions,
  RevokeAll,
  Remove,
}

fn users_url(
  q: &str,
  page_num: i64,
  extra: Option<(&str, &str)>,
  erro: Option<&str>,
) -> String {
  let mut parts = Vec::new();
  if !q.is_empty() {
    parts.push(format!("q={}", encode_query(q)));
  }
  if page_num > 1 {
    parts.push(format!("page={page_num}"));
  }
  if let Some((key, value)) = extra {
    parts.push(format!("{key}={}", encode_query(value)));
  }
  if let Some(erro) = erro.map(str::trim).filter(|s| !s.is_empty()) {
    parts.push(format!("erro={}", encode_query(erro)));
  }
  if parts.is_empty() {
    "/admin/usuarios".to_string()
  } else {
    format!("/admin/usuarios?{}", parts.join("&"))
  }
}

fn form_q(raw: &Option<String>) -> String {
  raw.clone().unwrap_or_default()
}

fn form_page(raw: &Option<String>) -> i64 {
  raw
    .as_deref()
    .and_then(|value| value.parse().ok())
    .filter(|number| *number > 0)
    .unwrap_or(1)
}

fn admin_error_message(err: &topcoat::Error) -> String {
  let msg = err.to_string().to_ascii_lowercase();
  if msg.contains("cannot ban yourself") {
    "Você não pode banir a si mesmo.".into()
  } else if msg.contains("cannot remove yourself") {
    "Você não pode remover a si mesmo.".into()
  } else if msg.contains("cannot impersonate yourself") {
    "Você não pode atuar como si mesmo.".into()
  } else if msg.contains("already impersonating") {
    "Você já está atuando como outro usuário.".into()
  } else if msg.contains("banned user") {
    "Não é possível atuar como um usuário banido.".into()
  } else if msg.contains("name") {
    "Informe o nome.".into()
  } else if msg.contains("role") {
    "Papel inválido.".into()
  } else if msg.contains("not found") {
    "Não encontrado.".into()
  } else if msg.contains("forbidden") {
    "Você não tem permissão para esta ação.".into()
  } else {
    "Não foi possível concluir a operação. Tente novamente.".into()
  }
}

fn back(q: &str, page_num: i64) -> SeeOther {
  see_other(users_url(q, page_num, None, None))
}

fn fail(q: &str, page_num: i64, err: &topcoat::Error) -> SeeOther {
  see_other(users_url(
    q,
    page_num,
    None,
    Some(&admin_error_message(err)),
  ))
}

fn invalid_id(q: &str, page_num: i64) -> SeeOther {
  see_other(users_url(
    q,
    page_num,
    None,
    Some("Identificador inválido."),
  ))
}

async fn require_admin(cx: &Cx) -> Result<SessionUser> {
  let su = require_user(cx).await?;
  if !service::is_admin(su.user.role.as_deref()) {
    return Err(see_other("/dashboard").into());
  }
  Ok(su)
}

fn parse_uuid(raw: &str) -> Option<Uuid> {
  Uuid::parse_str(raw.trim()).ok()
}

fn role_label(role: Option<&str>) -> &'static str {
  let role = role.unwrap_or("user");
  if role.eq_ignore_ascii_case("admin") {
    "Administrador"
  } else if role.eq_ignore_ascii_case("moderator") {
    "Moderador"
  } else {
    "Usuário"
  }
}

fn role_variant(role: Option<&str>) -> BadgeVariant {
  let role = role.unwrap_or("user");
  if role.eq_ignore_ascii_case("admin") {
    BadgeVariant::Primary
  } else if role.eq_ignore_ascii_case("moderator") {
    BadgeVariant::Outline
  } else {
    BadgeVariant::Secondary
  }
}

fn action_href(q: &str, page_num: i64, key: &str, id: &str) -> String {
  users_url(q, page_num, Some((key, id)), None)
}

const USER_QUERY_MAX: usize = 80;

fn clamp_user_query(raw: &str) -> String {
  raw.trim().chars().take(USER_QUERY_MAX).collect()
}

fn clamp_user_page(page_num: i64) -> i64 {
  page_num.clamp(1, 10_000)
}

/// Search box, user table, and pagination. Re-renders on the server when the
/// query or page changes. Dialogs stay on the page and open through links
/// that carry the current query.
///
/// The shard endpoint does not run the page guard, so this checks the admin
/// role itself. Restored signal values are treated as user input.
#[shard("/admin/usuarios/lista")]
async fn user_directory(
  cx: &Cx,
  q: String,
  initial_page: i64,
) -> Result<impl View> {
  let actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let q = signal(cx, || clamp_user_query(&q));
  let current_page = signal(cx, || clamp_user_page(initial_page));

  let query = clamp_user_query(&q.get());
  let mut page_num = clamp_user_page(current_page.get());
  let page_size = service::ADMIN_USER_PAGE_SIZE;
  let search = Some(query.as_str()).filter(|value| !value.is_empty());
  let mut list = service::list_admin_users(
    pool,
    &actor,
    search,
    page_size,
    (page_num - 1) * page_size,
  )
  .await?;

  let total_pages = if list.total == 0 {
    0
  } else {
    (list.total + page_size - 1) / page_size
  };
  if total_pages > 0 && page_num > total_pages {
    page_num = total_pages;
    list = service::list_admin_users(
      pool,
      &actor,
      search,
      page_size,
      (page_num - 1) * page_size,
    )
    .await?;
  }

  let page_value = page_num.to_string();
  let rows: Vec<UserRow> = list
    .users
    .iter()
    .map(|user| {
      let id = user.id.to_string();
      let menu_id = format!("user-actions-{id}");
      let menu_target = menu_id.clone();
      let anchor = format!("--user-actions-{id}");
      let is_self = user.id == actor.user.id;
      let target_admin = service::is_admin(user.role.as_deref());
      UserRow {
        menu_id,
        menu_target,
        trigger_style: format!("anchor-name: {anchor}"),
        panel_style: format!(
          "position-anchor: {anchor}; margin: 0; inset: auto; top: calc(anchor(bottom) + 4px); right: anchor(right)"
        ),
        q: query.clone(),
        page: page_value.clone(),
        name: user.name.clone(),
        email: user.email.clone(),
        role_label: role_label(user.role.as_deref()),
        role_variant: role_variant(user.role.as_deref()),
        banned: user.banned,
        show_ban: !is_self && !user.banned,
        show_unban: !is_self && user.banned,
        show_impersonate: !is_self && !target_admin,
        show_remove: !is_self,
        href_papel: action_href(&query, page_num, "papel", &id),
        href_editar: action_href(&query, page_num, "editar", &id),
        href_banir: action_href(&query, page_num, "banir", &id),
        href_desbanir: action_href(&query, page_num, "desbanir", &id),
        href_sessoes: action_href(&query, page_num, "sessoes", &id),
        href_revogar: action_href(&query, page_num, "revogar_todas", &id),
        href_remover: action_href(&query, page_num, "remover", &id),
        id,
      }
    })
    .collect();
  let is_empty = rows.is_empty();
  let show_pagination = total_pages > 1;
  let has_prev = page_num > 1;
  let has_next = total_pages > 0 && page_num < total_pages;
  let prev_page = page_num - 1;
  let next_page = page_num + 1;
  let page_label = if total_pages == 0 {
    "Página 1 de 1".to_string()
  } else {
    format!("Página {page_num} de {total_pages}")
  };

  Ok(view! {
      <div class="mb-6 max-w-xl">
          input(attrs: attributes! {
              id="user-search"
              type="search"
              placeholder="Buscar por nome ou e-mail"
              aria-label="Buscar usuários"
              :value=$(q.get())
              @input=$(|e: Event| {
                  q.set(e.target.value);
                  current_page.set(1i64);
              })
          })
      </div>

      if is_empty {
          <p class="text-sm text-muted-foreground">"Nenhum usuário encontrado."</p>
      } else {
          table(
              table_header(
                  table_row(
                      table_head("Nome")
                      table_head("Email")
                      table_head("Papel")
                      table_head("Banido")
                      table_head("Ações")
                  )
              )
              table_body(
                  #[key(row.id.clone())]
                  for row in rows {
                      table_row(
                          table_cell(<span class="font-medium">(row.name)</span>)
                          table_cell(<span class="text-muted-foreground">(row.email)</span>)
                          table_cell(badge(variant: row.role_variant, (row.role_label)))
                          table_cell(
                              if row.banned {
                                  badge(variant: BadgeVariant::Destructive, "Banido")
                              } else {
                                  <span class="text-muted-foreground">"Não"</span>
                              }
                          )
                          table_cell(
                              <button
                                  type="button"
                                  popovertarget=(row.menu_target)
                                  style=(row.trigger_style)
                                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Icon))
                                  aria-label="Abrir menu"
                              >
                                  icon(data: iconify_icon!("lucide:more-horizontal"))
                              </button>
                              <div
                                  id=(row.menu_id)
                                  popover="auto"
                                  style=(row.panel_style)
                                  class="min-w-56 rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-sm"
                              >
                                  <a href=(row.href_papel) class=(ACTION_LINK)>"Definir papel"</a>
                                  <a href=(row.href_editar) class=(ACTION_LINK)>"Atualizar usuário"</a>
                                  if row.show_unban || row.show_ban {
                                      <hr class="-mx-1 my-1 border-border">
                                      if row.show_unban {
                                          <a href=(row.href_desbanir) class=(ACTION_LINK)>"Revogar banimento"</a>
                                      } else {
                                          <a href=(row.href_banir) class=(ACTION_LINK)>"Banir usuário"</a>
                                      }
                                  }
                                  <a href=(row.href_sessoes) class=(ACTION_LINK)>"Listar sessões"</a>
                                  <a href=(row.href_revogar) class=(ACTION_LINK)>"Revogar todas as sessões"</a>
                                  if row.show_impersonate {
                                      <hr class="-mx-1 my-1 border-border">
                                      <form method="post" action="/admin/usuarios/atuar">
                                          <input type="hidden" name="user_id" value=(row.id)>
                                          <input type="hidden" name="q" value=(row.q)>
                                          <input type="hidden" name="page" value=(row.page)>
                                          <button type="submit" class=(ACTION_LINK)>"Atuar como usuário"</button>
                                      </form>
                                  }
                                  if row.show_remove {
                                      <hr class="-mx-1 my-1 border-border">
                                      <a href=(row.href_remover) class=(ACTION_DANGER)>"Remover usuário"</a>
                                  }
                              </div>
                          )
                      )
                  }
              )
          )
      }

      if show_pagination {
          <div class="mt-6">
              pagination(
                  pagination_content(
                      if has_prev {
                          pagination_item(
                              <button
                                  type="button"
                                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))
                                  @click=$(|_e: Event| current_page.set(prev_page))
                              >
                                  "Anterior"
                              </button>
                          )
                      }
                      pagination_item(
                          <span class="px-3 text-sm text-muted-foreground">(page_label)</span>
                      )
                      if has_next {
                          pagination_item(
                              <button
                                  type="button"
                                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))
                                  @click=$(|_e: Event| current_page.set(next_page))
                              >
                                  "Próxima"
                              </button>
                          )
                      }
                  )
              )
          </div>
      }
  })
}

#[page(GET "/admin/usuarios")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<UsersQuery>(cx)?;

  let q = clamp_user_query(query.q.as_deref().unwrap_or(""));
  let page_num = form_page(&query.page);

  let panel_raw = query
    .papel
    .as_deref()
    .or(query.editar.as_deref())
    .or(query.banir.as_deref())
    .or(query.desbanir.as_deref())
    .or(query.sessoes.as_deref())
    .or(query.revogar_todas.as_deref())
    .or(query.remover.as_deref())
    .map(str::trim)
    .filter(|value| !value.is_empty());

  let panel_kind = if query.papel.is_some() {
    Some(PanelKind::Role)
  } else if query.editar.is_some() {
    Some(PanelKind::Edit)
  } else if query.banir.is_some() {
    Some(PanelKind::Ban)
  } else if query.desbanir.is_some() {
    Some(PanelKind::Unban)
  } else if query.sessoes.is_some() {
    Some(PanelKind::Sessions)
  } else if query.revogar_todas.is_some() {
    Some(PanelKind::RevokeAll)
  } else if query.remover.is_some() {
    Some(PanelKind::Remove)
  } else {
    None
  };

  let panel_user = match panel_raw.and_then(parse_uuid) {
    Some(id) => service::load_user(pool, id).await.ok(),
    None => None,
  };

  let mut error_message = query.erro.clone().filter(|value| !value.is_empty());
  if panel_raw.is_some() && panel_user.is_none() && error_message.is_none() {
    error_message = Some("Usuário não encontrado.".to_string());
  }

  let page_value = page_num.to_string();
  let dialog_id = panel_user
    .as_ref()
    .map(|user| user.id.to_string())
    .unwrap_or_default();
  let edit_name = panel_user
    .as_ref()
    .map(|user| user.name.clone())
    .unwrap_or_default();
  let subject_name = edit_name.clone();
  let stored_role = panel_user
    .as_ref()
    .and_then(|user| user.role.clone())
    .unwrap_or_else(|| "user".to_string());
  let role_admin = stored_role.eq_ignore_ascii_case("admin");
  let role_moderator = stored_role.eq_ignore_ascii_case("moderator");
  let role_user = !role_admin && !role_moderator;

  let mut sessions = Vec::new();
  if matches!(panel_kind, Some(PanelKind::Sessions)) && panel_user.is_some() {
    let user_id = panel_user.as_ref().map(|user| user.id).expect("panel user");
    sessions = service::list_admin_user_sessions(pool, &actor, user_id)
      .await?
      .into_iter()
      .map(|row| {
        let id = row.id.to_string();
        let short_id = format!("{}…", &id[..8]);
        SessionView {
          id,
          user_id: dialog_id.clone(),
          q: q.clone(),
          page: page_value.clone(),
          short_id,
          created_at: row.created_at,
          expires_at: row.expires_at,
          ip_address: row.ip_address.unwrap_or_else(|| "—".to_string()),
          user_agent: row.user_agent.unwrap_or_else(|| "—".to_string()),
        }
      })
      .collect();
  }
  let has_sessions = !sessions.is_empty();

  let show_dialog = panel_user.is_some();
  let role_open = show_dialog && matches!(panel_kind, Some(PanelKind::Role));
  let edit_open = show_dialog && matches!(panel_kind, Some(PanelKind::Edit));
  let ban_open = show_dialog && matches!(panel_kind, Some(PanelKind::Ban));
  let unban_open = show_dialog && matches!(panel_kind, Some(PanelKind::Unban));
  let sessions_open =
    show_dialog && matches!(panel_kind, Some(PanelKind::Sessions));
  let revoke_all_open =
    show_dialog && matches!(panel_kind, Some(PanelKind::RevokeAll));
  let remove_open =
    show_dialog && matches!(panel_kind, Some(PanelKind::Remove));

  let ban_text = format!(
    "Isso impedirá {subject_name} de fazer login e revogará todas as suas sessões. Você pode informar um motivo opcionalmente."
  );
  let unban_text =
    format!("Isso permitirá que {subject_name} faça login novamente.");
  let revoke_text = format!(
    "Isso encerrará a sessão de {subject_name} em todos os dispositivos."
  );
  let remove_text = format!(
    "Isso excluirá permanentemente {subject_name} e seus dados. Esta ação não pode ser desfeita."
  );
  let sessions_title = format!("Sessões de {subject_name}");
  let list_href = users_url(&q, page_num, None, None);
  let revoke_all_href = action_href(&q, page_num, "revogar_todas", &dialog_id);
  let hidden_q = q.clone();
  let hidden_page = page_value;
  let cancel_href = list_href;

  Ok(view! {
      <div class="p-8">
              <h1 class="mb-2 text-2xl font-semibold tracking-tight">"Gerenciamento de usuários"</h1>
              <p class="mb-8 text-muted-foreground">"Gerencie papéis, banimentos, sessões e representação de usuários."</p>

              if let Some(message) = error_message {
                  <div class="mb-6">
                      alert(
                          variant: AlertVariant::Destructive,
                          alert_title((message))
                      )
                  </div>
              }

              user_directory(q: q, initial_page: page_num)

              if role_open {
                  dialog(
                      open: true,
                      attrs: attributes! { aria-label="Definir papel" },
                      dialog_content(
                          dialog_header(dialog_title("Definir papel"))
                          <form method="post" action="/admin/usuarios/papel" class="flex flex-col gap-4">
                              <input type="hidden" name="user_id" value=(dialog_id)>
                              <input type="hidden" name="q" value=(hidden_q)>
                              <input type="hidden" name="page" value=(hidden_page)>
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="role" }, "Papel")
                                  select(
                                      attrs: attributes! { name="role" id="role" },
                                      <option value="admin" selected=(role_admin)>"Administrador"</option>
                                      <option value="moderator" selected=(role_moderator)>"Moderador"</option>
                                      <option value="user" selected=(role_user)>"Usuário"</option>
                                  )
                              </div>
                              dialog_footer(
                                  <a href=(cancel_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Cancelar"</a>
                                  button(
                                      variant: ButtonVariant::Primary,
                                      attrs: attributes! { type="submit" },
                                      "Salvar"
                                  )
                              )
                          </form>
                      )
                  )
              } else if edit_open {
                  dialog(
                      open: true,
                      attrs: attributes! { aria-label="Atualizar usuário" },
                      dialog_content(
                          dialog_header(dialog_title("Atualizar usuário"))
                          <form method="post" action="/admin/usuarios/nome" class="flex flex-col gap-4">
                              <input type="hidden" name="user_id" value=(dialog_id)>
                              <input type="hidden" name="q" value=(hidden_q)>
                              <input type="hidden" name="page" value=(hidden_page)>
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="name" }, "Nome")
                                  input(attrs: attributes! {
                                      type="text"
                                      name="name"
                                      id="name"
                                      value=(edit_name)
                                      placeholder="Nome do usuário"
                                      required=""
                                  })
                              </div>
                              dialog_footer(
                                  <a href=(cancel_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Cancelar"</a>
                                  button(
                                      variant: ButtonVariant::Primary,
                                      attrs: attributes! { type="submit" },
                                      "Salvar"
                                  )
                              )
                          </form>
                      )
                  )
              } else if ban_open {
                  alert_dialog(
                      open: true,
                      attrs: attributes! { aria-label="Banir usuário" },
                      dialog_content(
                          dialog_header(
                              dialog_title("Banir usuário")
                              dialog_description((ban_text))
                          )
                          <form method="post" action="/admin/usuarios/banir" class="flex flex-col gap-4">
                              <input type="hidden" name="user_id" value=(dialog_id)>
                              <input type="hidden" name="q" value=(hidden_q)>
                              <input type="hidden" name="page" value=(hidden_page)>
                              label(attrs: attributes! { for="ban-reason" class="sr-only" }, "Motivo (opcional)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="ban_reason"
                                  id="ban-reason"
                                  placeholder="Motivo (opcional)"
                              })
                              dialog_footer(
                                  <a href=(cancel_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Cancelar"</a>
                                  button(
                                      variant: ButtonVariant::Destructive,
                                      attrs: attributes! { type="submit" },
                                      "Banir usuário"
                                  )
                              )
                          </form>
                      )
                  )
              } else if unban_open {
                  alert_dialog(
                      open: true,
                      attrs: attributes! { aria-label="Revogar banimento" },
                      dialog_content(
                          dialog_header(
                              dialog_title("Revogar banimento")
                              dialog_description((unban_text))
                          )
                          <form method="post" action="/admin/usuarios/desbanir">
                              <input type="hidden" name="user_id" value=(dialog_id)>
                              <input type="hidden" name="q" value=(hidden_q)>
                              <input type="hidden" name="page" value=(hidden_page)>
                              dialog_footer(
                                  <a href=(cancel_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Cancelar"</a>
                                  button(
                                      variant: ButtonVariant::Primary,
                                      attrs: attributes! { type="submit" },
                                      "Revogar banimento"
                                  )
                              )
                          </form>
                      )
                  )
              } else if sessions_open {
                  dialog(
                      open: true,
                      attrs: attributes! { aria-label=(sessions_title.clone()) },
                      dialog_content(
                          dialog_header(dialog_title((sessions_title)))
                          if has_sessions {
                              <ul class="flex max-h-60 flex-col gap-2 overflow-y-auto">
                                  for session in sessions {
                                      <li class="flex items-start justify-between gap-3 rounded-md border border-border px-3 py-2 text-sm">
                                          <div class="min-w-0">
                                              <p class="font-mono text-muted-foreground">(session.short_id)</p>
                                              <p class="text-xs text-muted-foreground">"Criada " (session.created_at)</p>
                                              <p class="text-xs text-muted-foreground">"Expira " (session.expires_at)</p>
                                              <p class="truncate text-xs text-muted-foreground">(session.ip_address)</p>
                                              <p class="truncate text-xs text-muted-foreground">(session.user_agent)</p>
                                          </div>
                                          <form method="post" action="/admin/usuarios/sessoes/revogar">
                                              <input type="hidden" name="session_id" value=(session.id)>
                                              <input type="hidden" name="user_id" value=(session.user_id)>
                                              <input type="hidden" name="q" value=(session.q)>
                                              <input type="hidden" name="page" value=(session.page)>
                                              button(
                                                  variant: ButtonVariant::Ghost,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" },
                                                  "Revogar"
                                              )
                                          </form>
                                      </li>
                                  }
                              </ul>
                          } else {
                              <p class="py-4 text-sm text-muted-foreground">"Nenhuma sessão ativa."</p>
                          }
                          dialog_footer(
                              <a href=(cancel_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Fechar"</a>
                              if has_sessions {
                                  <a href=(revoke_all_href) class=(button_variants(ButtonVariant::Destructive, ButtonSize::Md))>"Revogar todas as sessões"</a>
                              }
                          )
                      )
                  )
              } else if revoke_all_open {
                  alert_dialog(
                      open: true,
                      attrs: attributes! { aria-label="Revogar todas as sessões" },
                      dialog_content(
                          dialog_header(
                              dialog_title("Revogar todas as sessões")
                              dialog_description((revoke_text))
                          )
                          <form method="post" action="/admin/usuarios/sessoes/revogar-todas">
                              <input type="hidden" name="user_id" value=(dialog_id)>
                              <input type="hidden" name="q" value=(hidden_q)>
                              <input type="hidden" name="page" value=(hidden_page)>
                              dialog_footer(
                                  <a href=(cancel_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Cancelar"</a>
                                  button(
                                      variant: ButtonVariant::Destructive,
                                      attrs: attributes! { type="submit" },
                                      "Revogar todas"
                                  )
                              )
                          </form>
                      )
                  )
              } else if remove_open {
                  alert_dialog(
                      open: true,
                      attrs: attributes! { aria-label="Remover usuário" },
                      dialog_content(
                          dialog_header(
                              dialog_title("Remover usuário")
                              dialog_description((remove_text))
                          )
                          <form method="post" action="/admin/usuarios/remover">
                              <input type="hidden" name="user_id" value=(dialog_id)>
                              <input type="hidden" name="q" value=(hidden_q)>
                              <input type="hidden" name="page" value=(hidden_page)>
                              dialog_footer(
                                  <a href=(cancel_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Cancelar"</a>
                                  button(
                                      variant: ButtonVariant::Destructive,
                                      attrs: attributes! { type="submit" },
                                      "Remover usuário"
                                  )
                              )
                          </form>
                      )
                  )
              }
      </div>
  })
}

#[route(POST "/admin/usuarios/papel")]
async fn set_role(cx: &Cx, Form(body): Form<RoleForm>) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(user_id) = parse_uuid(&body.user_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  match service::set_user_role(pool, &actor, user_id, &body.role).await {
    Ok(_) => Ok(back(&q, page_num)),
    Err(err) => Ok(fail(&q, page_num, &err)),
  }
}

#[route(POST "/admin/usuarios/nome")]
async fn update_name(cx: &Cx, Form(body): Form<NameForm>) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(user_id) = parse_uuid(&body.user_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  match service::admin_update_user_name(pool, &actor, user_id, &body.name).await
  {
    Ok(_) => Ok(back(&q, page_num)),
    Err(err) => Ok(fail(&q, page_num, &err)),
  }
}

#[route(POST "/admin/usuarios/banir")]
async fn ban_user(cx: &Cx, Form(body): Form<BanForm>) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(user_id) = parse_uuid(&body.user_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  match service::ban_user(pool, &actor, user_id, body.ban_reason.as_deref())
    .await
  {
    Ok(_) => Ok(back(&q, page_num)),
    Err(err) => Ok(fail(&q, page_num, &err)),
  }
}

#[route(POST "/admin/usuarios/desbanir")]
async fn unban_user(cx: &Cx, Form(body): Form<IdForm>) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(user_id) = parse_uuid(&body.user_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  match service::unban_user(pool, &actor, user_id).await {
    Ok(_) => Ok(back(&q, page_num)),
    Err(err) => Ok(fail(&q, page_num, &err)),
  }
}

#[route(POST "/admin/usuarios/atuar")]
async fn impersonate(cx: &Cx, Form(body): Form<IdForm>) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(user_id) = parse_uuid(&body.user_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  match service::impersonate_user(cx, pool, &actor, user_id).await {
    Ok(_) => Ok(see_other("/dashboard")),
    Err(err) => {
      let message = if err.to_string() == "forbidden" {
        "Não é possível atuar como um administrador.".to_string()
      } else {
        admin_error_message(&err)
      };
      Ok(see_other(users_url(&q, page_num, None, Some(&message))))
    }
  }
}

#[route(POST "/admin/usuarios/sessoes/revogar")]
async fn revoke_session(
  cx: &Cx,
  Form(body): Form<RevokeForm>,
) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(session_id) = parse_uuid(&body.session_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  let back_to =
    users_url(&q, page_num, Some(("sessoes", body.user_id.trim())), None);
  match service::revoke_admin_session(pool, &actor, session_id).await {
    Ok(_) => Ok(see_other(back_to)),
    Err(err) => Ok(see_other(users_url(
      &q,
      page_num,
      Some(("sessoes", body.user_id.trim())),
      Some(&admin_error_message(&err)),
    ))),
  }
}

#[route(POST "/admin/usuarios/sessoes/revogar-todas")]
async fn revoke_sessions(
  cx: &Cx,
  Form(body): Form<IdForm>,
) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(user_id) = parse_uuid(&body.user_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  match service::revoke_admin_user_sessions(pool, &actor, user_id).await {
    Ok(_) => Ok(back(&q, page_num)),
    Err(err) => Ok(fail(&q, page_num, &err)),
  }
}

#[route(POST "/admin/usuarios/remover")]
async fn remove_user(cx: &Cx, Form(body): Form<IdForm>) -> Result<SeeOther> {
  let actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let page_num = form_page(&body.page);
  let Some(user_id) = parse_uuid(&body.user_id) else {
    return Ok(invalid_id(&q, page_num));
  };
  let pool = app_context::<PgPool>(cx);
  match service::remove_user(pool, &actor, user_id).await {
    Ok(_) => Ok(back(&q, page_num)),
    Err(err) => Ok(fail(&q, page_num, &err)),
  }
}
