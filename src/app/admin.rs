pub mod configuracoes;
pub mod estoque;
pub mod pedidos;
pub mod produtos;
pub mod usuarios;

use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::page,
  view::{View, view},
};

use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
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
      <div class="flex min-h-screen">
          <aside class="w-60 shrink-0 border-r border-border bg-card p-6">
              <h2 class="mb-6 text-lg font-semibold">"Admin"</h2>
              <nav class="flex flex-col gap-1">
                  <a href="/admin" class="rounded-md bg-primary/10 px-3 py-2 text-sm font-medium text-primary">"Dashboard"</a>
                  <a href="/admin/usuarios" class="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-foreground/5">"Usuários"</a>
                  <a href="/admin/produtos" class="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-foreground/5">"Produtos"</a>
                  <a href="/admin/pedidos" class="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-foreground/5">"Pedidos"</a>
                  <a href="/admin/estoque" class="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-foreground/5">"Estoque"</a>
                  <a href="/admin/configuracoes" class="rounded-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-foreground/5">"Configurações"</a>
              </nav>
          </aside>
          <main class="flex-1 p-8">
              <h1 class="mb-2 text-2xl font-semibold tracking-tight">"Bem-vinda ao painel"</h1>
              <p class="mb-8 text-muted-foreground">"Gerencie produtos, estoque e pedidos da Linda Flor."</p>

              <div class="mb-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
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
                                          <a href="/admin/pedidos" class="font-mono text-primary">(order.id)</a>
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
          </main>
      </div>
  })
}
