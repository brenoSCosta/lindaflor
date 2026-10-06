pub mod configuracoes;
pub mod cupons;
pub mod estoque;
pub mod pedidos;
pub mod produtos;
pub mod usuarios;

use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{Body, Next, href, layer, page, response::Response},
  view::{View, view},
};

use crate::app::auth_helpers::require_admin;

use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::container;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

/// Defense-in-depth guard for `/admin` and below. Anonymous users go to
/// `/login`, non-admins to `/dashboard` (see `require_admin`).
///
/// Shard/procedure endpoints bypass page/layout guards, so handlers must
/// still call `require_admin` explicitly.
#[layer]
async fn admin_guard(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
  require_admin(cx).await?;
  next.run(cx, body).await
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);

  let total_products: i64 =
    sqlx::query_scalar!("SELECT COUNT(*) FROM products")
      .fetch_one(pool)
      .await?
      .unwrap_or(0);

  let total_orders: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM orders")
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

  let total_revenue: i64 = sqlx::query_scalar!(
        "SELECT COALESCE(SUM(total_cents), 0)::bigint FROM orders WHERE status NOT IN ('cancelled')"
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

  let low_stock_count: i64 =
    sqlx::query_scalar!("SELECT COUNT(*) FROM inventory WHERE quantity <= 5")
      .fetch_one(pool)
      .await?
      .unwrap_or(0);

  let recent_orders = sqlx::query!(
        "SELECT id::text AS \"id!\", guest_email, status::text AS \"status!\", total_cents, created_at FROM orders ORDER BY created_at DESC LIMIT 10"
    )
    .fetch_all(pool)
    .await?;

  Ok(view! {
      container(
              <h1 class="text-2xl font-semibold tracking-tight">"Bem-vinda ao painel"</h1>
              <p class="text-muted-foreground">"Gerencie produtos, estoque e pedidos da Linda Flor."</p>

              <div class="grid gap-4 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                  card(
                      card_content(
                          <p class="text-sm text-muted-foreground">"Total de Produtos"</p>
                          <p class="mt-2 text-3xl font-semibold">(total_products)</p>
                      )
                  )
                  card(
                      card_content(
                          <p class="text-sm text-muted-foreground">"Total de Pedidos"</p>
                          <p class="mt-2 text-3xl font-semibold">(total_orders)</p>
                      )
                  )
                  card(
                      card_content(
                          <p class="text-sm text-muted-foreground">"Receita Total"</p>
                          <p class="mt-2 text-3xl font-semibold">"R$ " (total_revenue / 100)</p>
                      )
                  )
                  card(
                      card_content(
                          <p class="text-sm text-muted-foreground">"Estoque Baixo"</p>
                          <p class="mt-2 text-3xl font-semibold text-destructive">(low_stock_count)</p>
                      )
                  )
              </div>

              card(
                  card_header(
                      card_title("Pedidos Recentes")
                  )
                  card_content(
                      table(
                          table_header(
                              table_row(
                                  table_head("ID")
                                  table_head("Cliente")
                                  table_head("Status")
                                  table_head("Total")
                                  table_head("Data")
                              )
                          )
                          table_body(
                              for order in recent_orders {
                                  table_row(
                                      table_cell(
                                          <a href=(href!(crate::app::admin::pedidos::id::page, crate::app::admin::pedidos::id::Id(order.id.clone()))) class="font-mono text-primary">(order.id)</a>
                                      )
                                      table_cell((order.guest_email))
                                      table_cell((order.status))
                                      table_cell("R$ " (order.total_cents / 100))
                                      table_cell(
                                          <span class="text-muted-foreground">(order.created_at.to_string())</span>
                                      )
                                  )
                              }
                          )
                      )
                  )
              )
      )
  })
}
