use std::collections::HashSet;

use serde::Deserialize;
use sqlx::{PgPool, Row};
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{SeeOther, see_other},
    href, page, query_params, route,
  },
  runtime::{Event, procedure, shard, signal},
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_user;
use crate::auth::service;
use crate::auth::user::SessionUser;
use crate::components::alert::{AlertVariant, alert, alert_title};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

#[derive(Deserialize)]
struct CreateCouponForm {
  code: String,
  discount_type: String,
  discount_value: String,
  min_subtotal_cents: Option<String>,
  max_discount_cents: Option<String>,
  usage_type: String,
  max_uses: Option<String>,
  per_user_limit: Option<String>,
  starts_at: Option<String>,
  expires_at: Option<String>,
  q: Option<String>,
}

#[derive(Deserialize)]
struct IdForm {
  coupon_id: String,
  q: Option<String>,
  cupom: Option<String>,
  uq: Option<String>,
}

#[derive(Deserialize)]
struct AssignForm {
  coupon_id: String,
  #[serde(default)]
  user_ids: Vec<String>,
  q: Option<String>,
  cupom: Option<String>,
  uq: Option<String>,
}

#[derive(Deserialize)]
struct UnassignForm {
  coupon_id: String,
  user_id: String,
  q: Option<String>,
  cupom: Option<String>,
  uq: Option<String>,
}

#[query_params(error = bad_request)]
struct CouponsQuery {
  q: Option<String>,
  erro: Option<String>,
  cupom: Option<String>,
  uq: Option<String>,
}

struct CouponListItem {
  id: String,
  code: String,
  discount_label: String,
  discount_variant: BadgeVariant,
  min_label: String,
  max_label: String,
  active: bool,
  status_label: &'static str,
  status_variant: BadgeVariant,
  validity: String,
  usage_label: String,
  uses_count: i64,
  assignee_count: i64,
  manage_href: String,
}

struct AssignedUser {
  id: String,
  name: String,
  email: String,
}

struct AssignableUser {
  id: String,
  name: String,
  email: String,
  assigned: bool,
}

async fn require_admin(cx: &Cx) -> Result<SessionUser> {
  let su = require_user(cx).await?;
  if !service::is_admin(su.user.role.as_deref()) {
    return Err(
      see_other(href!(crate::app::dashboard::page).resolve(cx)).into(),
    );
  }
  Ok(su)
}

fn coupons_url(
  cx: &Cx,
  q: &str,
  cupom: &str,
  uq: &str,
  erro: Option<&str>,
) -> String {
  let mut pairs = Vec::new();
  if !q.is_empty() {
    pairs.push(("q", q));
  }
  if !cupom.is_empty() {
    pairs.push(("cupom", cupom));
  }
  if !cupom.is_empty() && !uq.is_empty() {
    pairs.push(("uq", uq));
  }
  if let Some(erro) = erro.map(str::trim).filter(|s| !s.is_empty()) {
    pairs.push(("erro", erro));
  }
  href!(page).query(pairs).resolve(cx)
}

fn form_q(raw: &Option<String>) -> String {
  raw.clone().unwrap_or_default()
}

fn form_text(raw: &Option<String>) -> String {
  raw.as_deref().unwrap_or("").trim().to_string()
}

fn back(cx: &Cx, q: &str, cupom: &str, uq: &str) -> SeeOther {
  see_other(coupons_url(cx, q, cupom, uq, None))
}

fn fail(cx: &Cx, q: &str, cupom: &str, uq: &str, message: &str) -> SeeOther {
  see_other(coupons_url(cx, q, cupom, uq, Some(message)))
}

fn db_error_message(err: &sqlx::Error) -> String {
  let msg = err.to_string().to_ascii_lowercase();
  if msg.contains("duplicate") || msg.contains("unique") {
    "Este código já está em uso.".into()
  } else if msg.contains("does not exist") || msg.contains("undefined") {
    "Tabelas de cupons indisponíveis. Fale com o responsável pela migração."
      .into()
  } else {
    "Não foi possível concluir a operação. Tente novamente.".into()
  }
}

fn parse_uuid(raw: &str) -> Option<Uuid> {
  Uuid::parse_str(raw.trim()).ok()
}

fn normalize_code(raw: &str) -> String {
  raw.trim().to_uppercase()
}

fn format_cents(cents: i32) -> String {
  let reais = cents / 100;
  let centavos = (cents % 100).abs();
  format!("R$ {reais},{centavos:02}")
}

fn short_dt(raw: &str) -> String {
  raw.trim().replace('T', " ").chars().take(16).collect()
}

fn col_i32(row: &sqlx::postgres::PgRow, name: &str) -> i32 {
  row
    .try_get::<i32, _>(name)
    .or_else(|_| row.try_get::<i64, _>(name).map(|v| v as i32))
    .unwrap_or(0)
}

fn col_opt_i32(row: &sqlx::postgres::PgRow, name: &str) -> Option<i32> {
  row
    .try_get::<Option<i32>, _>(name)
    .ok()
    .flatten()
    .or_else(|| {
      row
        .try_get::<Option<i64>, _>(name)
        .ok()
        .flatten()
        .map(|v| v as i32)
    })
}

fn col_count(row: &sqlx::postgres::PgRow, name: &str) -> i64 {
  row
    .try_get::<i64, _>(name)
    .or_else(|_| row.try_get::<Option<i64>, _>(name).map(|v| v.unwrap_or(0)))
    .unwrap_or(0)
}

async fn load_coupons(
  cx: &Cx,
  pool: &PgPool,
  q: &str,
) -> Result<Vec<CouponListItem>, sqlx::Error> {
  let rows = sqlx::query(
    "SELECT c.id::text AS id, c.code, c.discount_type, c.discount_value, \
     c.min_subtotal_cents, c.max_discount_cents, c.active, \
     c.starts_at::text AS starts_at, c.expires_at::text AS expires_at, \
     c.usage_type, c.max_uses, c.per_user_limit, \
     (SELECT COUNT(*) FROM coupon_redemptions r WHERE r.coupon_id = c.id) AS uses_count, \
     (SELECT COUNT(*) FROM coupon_assignments a WHERE a.coupon_id = c.id) AS assignee_count \
     FROM coupons c \
     WHERE ($1 = '' OR c.code ILIKE '%' || $1 || '%') \
     ORDER BY c.created_at DESC",
  )
  .bind(q)
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| {
        let id: String = row.try_get("id").unwrap_or_default();
        let code: String = row.try_get("code").unwrap_or_default();
        let discount_type: String =
          row.try_get("discount_type").unwrap_or_default();
        let discount_value = col_i32(row, "discount_value");
        let min_subtotal = col_i32(row, "min_subtotal_cents");
        let max_discount = col_opt_i32(row, "max_discount_cents");
        let active: bool = row.try_get("active").unwrap_or(false);
        let starts_at: Option<String> = row.try_get("starts_at").ok().flatten();
        let expires_at: Option<String> =
          row.try_get("expires_at").ok().flatten();
        let usage_type: String = row.try_get("usage_type").unwrap_or_default();
        let max_uses = col_opt_i32(row, "max_uses");
        let uses_count = col_count(row, "uses_count");
        let assignee_count = col_count(row, "assignee_count");

        let (discount_label, discount_variant) =
          if discount_type.eq_ignore_ascii_case("percent") {
            (format!("{discount_value}%"), BadgeVariant::Primary)
          } else {
            (format_cents(discount_value), BadgeVariant::Secondary)
          };
        let validity = match (starts_at, expires_at) {
          (Some(s), Some(e)) => format!("{} → {}", short_dt(&s), short_dt(&e)),
          (Some(s), None) => format!("de {}", short_dt(&s)),
          (None, Some(e)) => format!("até {}", short_dt(&e)),
          (None, None) => "Sem validade".to_string(),
        };
        let usage_label = if usage_type.eq_ignore_ascii_case("unique") {
          "Único".to_string()
        } else if let Some(n) = max_uses {
          format!("Até {n} usos")
        } else {
          "Ilimitado".to_string()
        };
        CouponListItem {
          manage_href: coupons_url(cx, "", &id, "", None),
          id,
          code,
          discount_label,
          discount_variant,
          min_label: format_cents(min_subtotal),
          max_label: max_discount
            .map(format_cents)
            .unwrap_or_else(|| "—".to_string()),
          active,
          status_label: if active { "Ativo" } else { "Inativo" },
          status_variant: if active {
            BadgeVariant::Primary
          } else {
            BadgeVariant::Outline
          },
          validity,
          usage_label,
          uses_count,
          assignee_count,
        }
      })
      .collect(),
  )
}

async fn load_assigned(
  pool: &PgPool,
  coupon_id: Uuid,
) -> Result<Vec<AssignedUser>, sqlx::Error> {
  let rows = sqlx::query(
    "SELECT u.id::text AS id, u.name, u.email \
     FROM coupon_assignments a \
     INNER JOIN users u ON u.id = a.user_id \
     WHERE a.coupon_id = $1 \
     ORDER BY u.name ASC",
  )
  .bind(coupon_id)
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .iter()
      .map(|row| AssignedUser {
        id: row.try_get("id").unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        email: row.try_get("email").unwrap_or_default(),
      })
      .collect(),
  )
}

async fn assigned_ids(pool: &PgPool, coupon_id: Uuid) -> HashSet<String> {
  sqlx::query_scalar::<_, String>(
    "SELECT user_id::text FROM coupon_assignments WHERE coupon_id = $1",
  )
  .bind(coupon_id)
  .fetch_all(pool)
  .await
  .unwrap_or_default()
  .into_iter()
  .collect()
}

const COUPON_QUERY_MAX: usize = 64;

fn clamp_coupon_query(raw: &str) -> String {
  raw.trim().chars().take(COUPON_QUERY_MAX).collect()
}

/// User search with per-row checkboxes (`name="user_ids"`) for N-user
/// assignment. Lives inside the assign `<form>` on the page; the shard
/// endpoint does not run the page guard, so this checks the admin role
/// itself. Restored signal values are treated as user input.
#[shard("/admin/cupons/usuarios")]
async fn coupon_user_directory(
  cx: &Cx,
  q: String,
  coupon_id: String,
) -> Result<impl View> {
  let actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let q = signal(cx, || clamp_coupon_query(&q));

  let assigned = match parse_uuid(&coupon_id) {
    Some(id) => assigned_ids(pool, id).await,
    None => HashSet::new(),
  };

  let query = clamp_coupon_query(&q.get());
  let search = Some(query.as_str()).filter(|value| !value.is_empty());
  let list = service::list_admin_users(pool, &actor, search, 50, 0).await?;
  let users: Vec<AssignableUser> = list
    .users
    .iter()
    .map(|user| {
      let id = user.id.to_string();
      AssignableUser {
        assigned: assigned.contains(&id),
        id,
        name: user.name.clone(),
        email: user.email.clone(),
      }
    })
    .collect();
  let is_empty = users.is_empty();
  let total = list.total;

  Ok(view! {
      <div class="mb-4">
          input(attrs: attributes! {
              id="coupon-user-search"
              type="search"
              placeholder="Buscar usuário por nome ou e-mail"
              aria-label="Buscar usuários para atribuir"
              :value=$(q.get())
              @input=$(|e: Event| { q.set(e.target.value); })
          })
          <p class="mt-1 text-xs text-muted-foreground">
              "A busca redefine a seleção. Selecione e atribua em seguida. "(total)" usuário(s) encontrado(s)."
          </p>
      </div>

      if is_empty {
          <p class="text-sm text-muted-foreground">"Nenhum usuário encontrado."</p>
      } else {
          table(
              table_header(
                  table_row(
                      table_head("Atribuir")
                      table_head("Nome")
                      table_head("Email")
                      table_head("Situação")
                  )
              )
              table_body(
                  #[key(user.id.clone())]
                  for user in users {
                      table_row(
                          table_cell(
                              if user.assigned {
                                  checkbox(attrs: attributes! {
                                      name="user_ids"
                                      value=(user.id)
                                      checked=""
                                      disabled=""
                                  })
                              } else {
                                  checkbox(attrs: attributes! {
                                      name="user_ids"
                                      value=(user.id)
                                  })
                              }
                          )
                          table_cell(<span class="font-medium">(user.name)</span>)
                          table_cell(<span class="text-muted-foreground">(user.email)</span>)
                          table_cell(
                              if user.assigned {
                                  badge(variant: BadgeVariant::Secondary, "Atribuído")
                              } else {
                                  <span class="text-muted-foreground">"—"</span>
                              }
                          )
                      )
                  }
              )
          )
      }
  })
}

/// Coupon search + table. Re-renders on the server when the query changes
/// or after a toggle, without reloading the page. Toggle uses the
/// `toggle_coupon_proc` procedure and bumps a version signal to refresh the
/// shard in place (no `location.reload()`).
///
/// The shard endpoint does not run the page guard, so this checks the admin
/// role itself. Restored signal values are treated as user input.
#[shard("/admin/cupons/lista")]
async fn coupon_directory(cx: &Cx, q: String) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let q = signal(cx, || clamp_coupon_query(&q));
  let version = signal(cx, || 0u64);
  let _tick = version.get();

  let query = clamp_coupon_query(&q.get());
  let (coupons, load_error) = match load_coupons(cx, pool, &query).await {
    Ok(coupons) => (coupons, None),
    Err(err) => (Vec::new(), Some(db_error_message(&err))),
  };
  let is_empty = coupons.is_empty();

  Ok(view! {
      <div class="mb-4 max-w-xl">
          input(attrs: attributes! {
              id="coupon-search"
              type="search"
              placeholder="Buscar por código"
              aria-label="Buscar cupons"
              :value=$(q.get())
              @input=$(|e: Event| { q.set(e.target.value); })
          })
      </div>

      if let Some(message) = load_error {
          <div class="mb-6">
              alert(
                  variant: AlertVariant::Destructive,
                  alert_title((message))
              )
          </div>
      }

      if is_empty {
          <p class="text-sm text-muted-foreground">"Nenhum cupom cadastrado."</p>
      } else {
          table(
              table_header(
                  table_row(
                      table_head("Código")
                      table_head("Desconto")
                      table_head("Mínimo")
                      table_head("Máx.")
                      table_head("Uso")
                      table_head("Usos")
                      table_head("Atribuídos")
                      table_head("Validade")
                      table_head("Status")
                      table_head("Ações")
                  )
              )
              table_body(
                  #[key(row.id.clone())]
                  for row in coupons {
                      table_row(
                          table_cell(<span class="font-mono font-medium">(row.code)</span>)
                          table_cell(badge(variant: row.discount_variant, (row.discount_label)))
                          table_cell((row.min_label))
                          table_cell((row.max_label))
                          table_cell((row.usage_label))
                          table_cell((row.uses_count))
                          table_cell((row.assignee_count))
                          table_cell(<span class="text-muted-foreground">(row.validity)</span>)
                          table_cell(badge(variant: row.status_variant, (row.status_label)))
                          table_cell(
                              <div class="flex items-center gap-2">
                                  <a href=(row.manage_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Sm))>"Atribuir"</a>
                                  <form method="post" action=(href!(toggle_coupon)) class="inline">
                                      <input type="hidden" name="coupon_id" value=(row.id.clone())>
                                      {
                                          let cid = row.id.clone();
                                          if row.active {
                                              button(
                                                  variant: ButtonVariant::Ghost,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! {
                                                      type="submit"
                                                      @click=$(async |e: Event| {
                                                          e.prevent_default();
                                                          toggle_coupon_proc(cid.to_owned()).await;
                                                          version.set(version.get() + 1u64);
                                                      })
                                                  },
                                                  "Desativar"
                                              )
                                          } else {
                                              button(
                                                  variant: ButtonVariant::Ghost,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! {
                                                      type="submit"
                                                      @click=$(async |e: Event| {
                                                          e.prevent_default();
                                                          toggle_coupon_proc(cid.to_owned()).await;
                                                          version.set(version.get() + 1u64);
                                                      })
                                                  },
                                                  "Ativar"
                                              )
                                          }
                                      }
                                  </form>
                                  <form method="post" action=(href!(delete_coupon)) class="inline">
                                      <input type="hidden" name="coupon_id" value=(row.id.clone())>
                                      button(
                                          variant: ButtonVariant::Destructive,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! { type="submit" },
                                          "Excluir"
                                      )
                                  </form>
                              </div>
                          )
                      )
                  }
              )
          )
      }
  })
}

#[page(GET "/admin/cupons")]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<CouponsQuery>(cx)?;

  let q = clamp_coupon_query(query.q.as_deref().unwrap_or(""));
  let selected = query
    .cupom
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .unwrap_or("")
    .to_string();
  let uq = clamp_coupon_query(query.uq.as_deref().unwrap_or(""));
  let error_message = query.erro.clone().filter(|value| !value.is_empty());

  let (coupons, load_error) = match load_coupons(cx, pool, &q).await {
    Ok(coupons) => (coupons, None),
    Err(err) => (Vec::new(), Some(db_error_message(&err))),
  };
  let selected_coupon = coupons.iter().find(|row| row.id == selected);
  let selected_code = selected_coupon
    .map(|row| row.code.clone())
    .unwrap_or_default();
  let show_assign = selected_coupon.is_some();

  let assigned = match parse_uuid(&selected) {
    Some(id) => load_assigned(pool, id).await.unwrap_or_default(),
    None => Vec::new(),
  };
  let has_assigned = !assigned.is_empty();
  let hidden_q = q.clone();
  let assign_q = hidden_q.clone();
  let assign_cupom = selected.clone();
  let assign_uq = uq.clone();
  let dir_q = uq.clone();
  let dir_cupom = selected.clone();

  Ok(view! {
      container(
          <h1 class="text-2xl font-semibold tracking-tight">"Cupons"</h1>
          <p class="text-muted-foreground">"Crie cupons, acompanhe usos e atribua a usuários específicos."</p>

          if let Some(message) = error_message {
              <div class="mb-6">
                  alert(
                      variant: AlertVariant::Destructive,
                      alert_title((message))
                  )
              </div>
          }

          if let Some(message) = load_error {
              <div class="mb-6">
                  alert(
                      variant: AlertVariant::Destructive,
                      alert_title((message))
                  )
              </div>
          }

          <div class="mb-8">
              card(
                  card_header(card_title("Novo cupom"))
                  card_content(
                      <form method="post" action=(href!(create_coupon)) class="grid gap-4 @xl/page:grid-cols-2 ">
                          <input type="hidden" name="q" value=(hidden_q)>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="code" }, "Código")
                              input(attrs: attributes! {
                                  type="text"
                                  name="code"
                                  id="code"
                                  placeholder="VERAO10"
                                  required=""
                                  maxlength="32"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="discount_type" }, "Tipo de desconto")
                              select(
                                  attrs: attributes! { name="discount_type" id="discount_type" },
                                  <option value="percent">"Percentual (1-50%)"</option>
                                  <option value="fixed">"Valor fixo (centavos)"</option>
                              )
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="discount_value" }, "Valor do desconto")
                              input(attrs: attributes! {
                                  type="number"
                                  name="discount_value"
                                  id="discount_value"
                                  min="1"
                                  max="50"
                                  required=""
                                  placeholder="10"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="min_subtotal_cents" }, "Subtotal mínimo (centavos)")
                              input(attrs: attributes! {
                                  type="number"
                                  name="min_subtotal_cents"
                                  id="min_subtotal_cents"
                                  min="0"
                                  value="0"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="max_discount_cents" }, "Desconto máximo (centavos, opcional)")
                              input(attrs: attributes! {
                                  type="number"
                                  name="max_discount_cents"
                                  id="max_discount_cents"
                                  min="1"
                                  placeholder="Opcional"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="usage_type" }, "Uso")
                              select(
                                  attrs: attributes! { name="usage_type" id="usage_type" },
                                  <option value="unique">"Único (1 uso global, exige atribuição)"</option>
                                  <option value="unlimited">"Ilimitado (teto opcional)"</option>
                              )
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="max_uses" }, "Máx. usos (opcional)")
                              input(attrs: attributes! {
                                  type="number"
                                  name="max_uses"
                                  id="max_uses"
                                  min="1"
                                  placeholder="Opcional"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="per_user_limit" }, "Limite por usuário (opcional)")
                              input(attrs: attributes! {
                                  type="number"
                                  name="per_user_limit"
                                  id="per_user_limit"
                                  min="1"
                                  placeholder="Opcional"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="starts_at" }, "Início (opcional)")
                              input(attrs: attributes! {
                                  type="datetime-local"
                                  name="starts_at"
                                  id="starts_at"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="expires_at" }, "Expira em (opcional)")
                              input(attrs: attributes! {
                                  type="datetime-local"
                                  name="expires_at"
                                  id="expires_at"
                              })
                          </div>
                          <div class="space-y-2">
                              button(
                                  variant: ButtonVariant::Primary,
                                  attrs: attributes! { type="submit" },
                                  "Criar cupom"
                              )
                          </div>
                      </form>
                  )
              )
          </div>

          coupon_directory(q: q.clone())

          if show_assign {
              
                  card(
                      card_header(card_title((format!("Atribuir {selected_code}"))))
                      card_content(
                          <p class="text-sm text-muted-foreground">"Somente usuários logados atribuídos aqui podem usar este cupom no checkout."</p>
                          if has_assigned {
                              <ul class="flex flex-col gap-2">
                                  for user in assigned {
                                      <li class="flex items-center justify-between gap-3 rounded-md border border-border px-3 py-2 text-sm">
                                          <div class="min-w-0">
                                              <p class="font-medium">(user.name)</p>
                                              <p class="truncate text-xs text-muted-foreground">(user.email)</p>
                                          </div>
                                          <form method="post" action=(href!(unassign_coupon))>
                                              <input type="hidden" name="coupon_id" value=(assign_cupom.clone())>
                                              <input type="hidden" name="user_id" value=(user.id)>
                                              <input type="hidden" name="q" value=(assign_q.clone())>
                                              <input type="hidden" name="cupom" value=(assign_cupom.clone())>
                                              <input type="hidden" name="uq" value=(assign_uq.clone())>
                                              button(
                                                  variant: ButtonVariant::Ghost,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" },
                                                  "Remover"
                                              )
                                          </form>
                                      </li>
                                  }
                              </ul>
                          } else {
                              <p class="mb-6 text-sm text-muted-foreground">"Nenhum usuário atribuído ainda."</p>
                          }
                          <form method="post" action=(href!(assign_coupon))>
                              <input type="hidden" name="coupon_id" value=(assign_cupom.clone())>
                              <input type="hidden" name="q" value=(assign_q.clone())>
                              <input type="hidden" name="cupom" value=(assign_cupom.clone())>
                              <input type="hidden" name="uq" value=(assign_uq.clone())>
                              coupon_user_directory(q: dir_q, coupon_id: dir_cupom)
                              <div class="mt-4">
                                  button(
                                      variant: ButtonVariant::Primary,
                                      attrs: attributes! { type="submit" },
                                      "Atribuir selecionados"
                                  )
                              </div>
                          </form>
                      )
                  )
              
          }
      )
  })
}

struct ValidCoupon {
  code: String,
  discount_type: &'static str,
  discount_value: i32,
  min_subtotal_cents: i32,
  max_discount_cents: Option<i32>,
  usage_type: &'static str,
  max_uses: Option<i32>,
  per_user_limit: i32,
  starts_at: Option<String>,
  expires_at: Option<String>,
}

fn parse_optional_int(
  raw: &Option<String>,
  field: &str,
) -> Result<Option<i32>, String> {
  let text = form_text(raw);
  if text.is_empty() {
    return Ok(None);
  }
  text
    .parse::<i32>()
    .map(Some)
    .map_err(|_| format!("Campo {field} inválido."))
}

fn validate_create(body: &CreateCouponForm) -> Result<ValidCoupon, String> {
  let code = normalize_code(&body.code);
  if code.is_empty() {
    return Err("Informe o código do cupom.".to_string());
  }
  if code.len() > 32 {
    return Err("O código deve ter até 32 caracteres.".to_string());
  }

  let discount_type = body.discount_type.trim().to_lowercase();
  let discount_type: &'static str = if discount_type == "percent" {
    "percent"
  } else if discount_type == "fixed" {
    "fixed"
  } else {
    return Err("Tipo de desconto inválido.".to_string());
  };

  let discount_value = body
    .discount_value
    .trim()
    .parse::<i32>()
    .map_err(|_| "Valor do desconto inválido.".to_string())?;
  if discount_type == "percent" && !(1..=50).contains(&discount_value) {
    return Err("Percentual deve ser entre 1 e 50.".to_string());
  }
  if discount_type == "fixed" && discount_value <= 0 {
    return Err("Valor do desconto deve ser maior que zero.".to_string());
  }

  let min_subtotal_cents = if form_text(&body.min_subtotal_cents).is_empty() {
    0
  } else {
    parse_optional_int(&body.min_subtotal_cents, "subtotal mínimo")?
      .unwrap_or(0)
  };
  if min_subtotal_cents < 0 {
    return Err("Subtotal mínimo inválido.".to_string());
  }

  let max_discount_cents =
    parse_optional_int(&body.max_discount_cents, "desconto máximo")?;
  if max_discount_cents.is_some_and(|v| v <= 0) {
    return Err("Desconto máximo inválido.".to_string());
  }

  let usage_type = body.usage_type.trim().to_lowercase();
  let usage_type: &'static str = if usage_type == "unique" {
    "unique"
  } else if usage_type == "unlimited" {
    "unlimited"
  } else {
    return Err("Tipo de uso inválido.".to_string());
  };

  let max_uses = match usage_type {
    "unique" => Some(1),
    _ => {
      let uses = parse_optional_int(&body.max_uses, "máx. usos")?;
      if uses.is_some_and(|v| v < 1) {
        return Err("Máx. de usos inválido.".to_string());
      }
      uses
    }
  };

  let per_user_limit =
    parse_optional_int(&body.per_user_limit, "limite por usuário")?
      .unwrap_or(1);
  if per_user_limit < 1 {
    return Err("Limite por usuário inválido.".to_string());
  }

  let starts_at = form_text(&body.starts_at);
  let expires_at = form_text(&body.expires_at);
  let starts_at = if starts_at.is_empty() {
    None
  } else {
    Some(starts_at)
  };
  let expires_at = if expires_at.is_empty() {
    None
  } else {
    Some(expires_at)
  };
  if let (Some(start), Some(end)) = (starts_at.as_ref(), expires_at.as_ref())
    && start > end
  {
    return Err("Início deve ser anterior ao término.".to_string());
  }

  Ok(ValidCoupon {
    code,
    discount_type,
    discount_value,
    min_subtotal_cents,
    max_discount_cents,
    usage_type,
    max_uses,
    per_user_limit,
    starts_at,
    expires_at,
  })
}

#[route(POST "/admin/cupons/create")]
async fn create_coupon(
  cx: &Cx,
  Form(body): Form<CreateCouponForm>,
) -> Result<SeeOther> {
  let _actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let coupon = match validate_create(&body) {
    Ok(coupon) => coupon,
    Err(message) => return Ok(fail(cx, &q, "", "", &message)),
  };
  let pool = app_context::<PgPool>(cx);
  let inserted = sqlx::query(
    "INSERT INTO coupons (id, code, discount_type, discount_value, \
     min_subtotal_cents, max_discount_cents, active, starts_at, expires_at, \
     usage_type, max_uses, per_user_limit) \
     VALUES ($1, $2, $3, $4, $5, $6, true, \
     NULLIF($7, '')::timestamptz, NULLIF($8, '')::timestamptz, $9, $10, $11) \
     ON CONFLICT (code) DO NOTHING",
  )
  .bind(Uuid::now_v7())
  .bind(&coupon.code)
  .bind(coupon.discount_type)
  .bind(coupon.discount_value)
  .bind(coupon.min_subtotal_cents)
  .bind(coupon.max_discount_cents)
  .bind(coupon.starts_at.as_deref().unwrap_or(""))
  .bind(coupon.expires_at.as_deref().unwrap_or(""))
  .bind(coupon.usage_type)
  .bind(coupon.max_uses)
  .bind(coupon.per_user_limit)
  .execute(pool)
  .await;

  match inserted {
    Ok(done) if done.rows_affected() == 1 => Ok(back(cx, &q, "", "")),
    Ok(_) => Ok(fail(cx, &q, "", "", "Este código já está em uso.")),
    Err(err) => Ok(fail(cx, &q, "", "", &db_error_message(&err))),
  }
}

/// JS fast-path for the per-row toggle form above. The hidden `coupon_id`
/// input stays as the no-JS fallback; the browser passes the id directly.
#[procedure("/admin/cupons/toggle-proc")]
async fn toggle_coupon_proc(cx: &Cx, coupon_id: String) -> Result<bool> {
  let _actor = require_admin(cx).await?;
  let Some(coupon_id) = parse_uuid(&coupon_id) else {
    return Ok(false);
  };
  let pool = app_context::<PgPool>(cx);
  match sqlx::query("UPDATE coupons SET active = NOT active WHERE id = $1")
    .bind(coupon_id)
    .execute(pool)
    .await
  {
    Ok(done) if done.rows_affected() == 1 => Ok(true),
    _ => Ok(false),
  }
}

#[route(POST "/admin/cupons/toggle")]
async fn toggle_coupon(cx: &Cx, Form(body): Form<IdForm>) -> Result<SeeOther> {
  let _actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let cupom = form_text(&body.cupom);
  let uq = form_text(&body.uq);
  let Some(coupon_id) = parse_uuid(&body.coupon_id) else {
    return Ok(fail(cx, &q, &cupom, &uq, "Cupom inválido."));
  };
  let pool = app_context::<PgPool>(cx);
  match sqlx::query("UPDATE coupons SET active = NOT active WHERE id = $1")
    .bind(coupon_id)
    .execute(pool)
    .await
  {
    Ok(done) if done.rows_affected() == 1 => Ok(back(cx, &q, &cupom, &uq)),
    Ok(_) => Ok(fail(cx, &q, &cupom, &uq, "Cupom não encontrado.")),
    Err(err) => Ok(fail(cx, &q, &cupom, &uq, &db_error_message(&err))),
  }
}

#[route(POST "/admin/cupons/delete")]
async fn delete_coupon(cx: &Cx, Form(body): Form<IdForm>) -> Result<SeeOther> {
  let _actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let cupom = form_text(&body.cupom);
  let uq = form_text(&body.uq);
  let Some(coupon_id) = parse_uuid(&body.coupon_id) else {
    return Ok(fail(cx, &q, &cupom, &uq, "Cupom inválido."));
  };
  let pool = app_context::<PgPool>(cx);
  match sqlx::query("DELETE FROM coupons WHERE id = $1")
    .bind(coupon_id)
    .execute(pool)
    .await
  {
    Ok(done) if done.rows_affected() == 1 => {
      let keep = if cupom == body.coupon_id.trim() {
        ""
      } else {
        &cupom
      };
      Ok(back(cx, &q, keep, &uq))
    }
    Ok(_) => Ok(fail(cx, &q, &cupom, &uq, "Cupom não encontrado.")),
    Err(err) => Ok(fail(cx, &q, &cupom, &uq, &db_error_message(&err))),
  }
}

#[route(POST "/admin/cupons/assign")]
async fn assign_coupon(
  cx: &Cx,
  Form(body): Form<AssignForm>,
) -> Result<SeeOther> {
  let _actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let cupom = form_text(&body.cupom);
  let uq = form_text(&body.uq);
  let Some(coupon_id) = parse_uuid(&body.coupon_id) else {
    return Ok(fail(cx, &q, &cupom, &uq, "Cupom inválido."));
  };
  let user_ids: Vec<Uuid> = body
    .user_ids
    .iter()
    .filter_map(|raw| parse_uuid(raw))
    .collect();
  if user_ids.is_empty() {
    return Ok(fail(cx, &q, &cupom, &uq, "Selecione ao menos um usuário."));
  }
  let pool = app_context::<PgPool>(cx);
  match sqlx::query(
    "INSERT INTO coupon_assignments (coupon_id, user_id) \
     SELECT $1, unnest($2::uuid[]) \
     ON CONFLICT DO NOTHING",
  )
  .bind(coupon_id)
  .bind(&user_ids)
  .execute(pool)
  .await
  {
    Ok(_) => Ok(back(cx, &q, &cupom, &uq)),
    Err(err) => Ok(fail(cx, &q, &cupom, &uq, &db_error_message(&err))),
  }
}

#[route(POST "/admin/cupons/unassign")]
async fn unassign_coupon(
  cx: &Cx,
  Form(body): Form<UnassignForm>,
) -> Result<SeeOther> {
  let _actor = require_admin(cx).await?;
  let q = form_q(&body.q);
  let cupom = form_text(&body.cupom);
  let uq = form_text(&body.uq);
  let (Some(coupon_id), Some(user_id)) =
    (parse_uuid(&body.coupon_id), parse_uuid(&body.user_id))
  else {
    return Ok(fail(cx, &q, &cupom, &uq, "Identificador inválido."));
  };
  let pool = app_context::<PgPool>(cx);
  match sqlx::query(
    "DELETE FROM coupon_assignments WHERE coupon_id = $1 AND user_id = $2",
  )
  .bind(coupon_id)
  .bind(user_id)
  .execute(pool)
  .await
  {
    Ok(_) => Ok(back(cx, &q, &cupom, &uq)),
    Err(err) => Ok(fail(cx, &q, &cupom, &uq, &db_error_message(&err))),
  }
}
