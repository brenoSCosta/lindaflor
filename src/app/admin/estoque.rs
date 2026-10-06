use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page, query_params},
  runtime::{Event, shard, signal},
  view::{View, attributes, view},
};

use crate::app::auth_helpers::require_user;
use crate::auth::service;
use crate::auth::user::SessionUser;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::container;
use crate::components::input::input;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

#[query_params(error = bad_request)]
struct StockQuery {
  q: Option<String>,
}

const STOCK_QUERY_MAX: usize = 80;

fn clamp_stock_query(raw: &str) -> String {
  raw.trim().chars().take(STOCK_QUERY_MAX).collect()
}

async fn require_admin(cx: &Cx) -> Result<SessionUser> {
  let su = require_user(cx).await?;
  if !service::is_admin(su.user.role.as_deref()) {
    return Err(
      topcoat::router::error::see_other(
        href!(crate::app::dashboard::page).resolve(cx),
      )
      .into(),
    );
  }
  Ok(su)
}

/// Stock search. Re-renders on the server when the query changes, without
/// reloading the page.
///
/// The shard endpoint does not run the page guard, so this checks the admin
/// role itself. Restored signal values are treated as user input.
#[shard("/admin/estoque/lista")]
async fn stock_directory(cx: &Cx, q: String) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let q = signal(cx, || clamp_stock_query(&q));
  let query = clamp_stock_query(&q.get());

  let inventory = sqlx::query!(
    "SELECT i.id, i.variant_id, i.warehouse_id, i.quantity, i.reserved,
                p.name as product_name, pv.sku, pv.size::text as size, pv.color,
                w.name as warehouse_name, w.code as warehouse_code
         FROM inventory i
         JOIN product_variants pv ON pv.id = i.variant_id
         JOIN products p ON p.id = pv.product_id
         JOIN warehouses w ON w.id = i.warehouse_id
         WHERE ($1 = '' OR p.name ILIKE '%' || $1 || '%' OR pv.sku ILIKE '%' || $1 || '%')
         ORDER BY p.name, pv.sku",
    query,
  )
  .fetch_all(pool)
  .await?;

  let warehouses = sqlx::query!(
    "SELECT id, code, name, is_default FROM warehouses ORDER BY name"
  )
  .fetch_all(pool)
  .await?;

  let is_empty = inventory.is_empty();

  Ok(view! {
          input(attrs: attributes! {
              id="stock-search"
              type="search"
              placeholder="Buscar por produto ou SKU"
              aria-label="Buscar estoque"
              :value=$(q.get())
              @input=$(|e: Event| { q.set(e.target.value); })
          })

      <div class="flex flex-col gap-6">
          card(
              card_header(card_title("Saldo por Depósito"))
              card_content(
                  if is_empty {
                      <p class="text-sm text-muted-foreground">"Nenhum item encontrado."</p>
                  } else {
                      table(
                          table_header(
                              table_row(
                                  table_head("Produto")
                                  table_head("SKU")
                                  table_head("Depósito")
                                  table_head("Tamanho")
                                  table_head("Qtd")
                                  table_head("Reservado")
                                  table_head("Disponível")
                              )
                          )
                          table_body(
                              #[key(item.id.to_string())]
                              for item in inventory {
                                  table_row(
                                      table_cell(<span class="font-medium">(item.product_name)</span>)
                                      table_cell((item.sku))
                                      table_cell((item.warehouse_name))
                                      table_cell((item.size))
                                      table_cell((item.quantity))
                                      table_cell((item.reserved))
                                      table_cell(
                                          <span class="font-medium">(item.quantity - item.reserved)</span>
                                      )
                                  )
                              }
                          )
                      )
                  }
              )
          )
          card(
              card_header(card_title("Depósitos"))
              card_content(
                  table(
                      table_header(
                          table_row(
                              table_head("Código")
                              table_head("Nome")
                              table_head("Padrão")
                          )
                      )
                      table_body(
                          for warehouse in warehouses {
                              table_row(
                                  table_cell(
                                      <span class="font-mono">(warehouse.code)</span>
                                  )
                                  table_cell(<span class="font-medium">(warehouse.name)</span>)
                                  table_cell(
                                      if warehouse.is_default {
                                          badge(variant: BadgeVariant::Secondary, "Sim")
                                      } else {
                                          <span class="text-muted-foreground">"—"</span>
                                      }
                                  )
                              )
                          }
                      )
                  )
              )
          )
      </div>
  })
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let query = query_params::<StockQuery>(cx)?;
  let q = clamp_stock_query(query.q.as_deref().unwrap_or(""));

  Ok(view! {
      container(
          <h1 class="text-2xl font-semibold tracking-tight">"Estoque"</h1>
          <p class="text-muted-foreground">"Saldo por depósito, entradas, transferências e sincronização CSV."</p>

          stock_directory(q: q)
      )
  })
}
