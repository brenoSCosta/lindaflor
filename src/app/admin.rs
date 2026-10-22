pub mod avaliacoes;
pub mod banners;
pub mod configuracoes;
pub mod cupons;
pub mod estoque;
pub mod newsletter;
pub mod pedidos;
pub mod produtos;
pub mod sets;
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

/// Variants of active products whose available stock (`quantity - reserved`
/// summed over active warehouses; no inventory rows counts as 0) is at or
/// below their own `low_stock_threshold`.
pub async fn count_low_stock_variants<'e>(
  executor: impl sqlx::PgExecutor<'e>,
) -> Result<i64, sqlx::Error> {
  let count = sqlx::query_scalar!(
    "SELECT COUNT(*) FROM (
       SELECT v.id
       FROM product_variants v
       INNER JOIN products p ON p.id = v.product_id
       LEFT JOIN inventory i
         ON i.variant_id = v.id
        AND i.warehouse_id IN (SELECT id FROM warehouses WHERE active)
       WHERE p.active
       GROUP BY v.id, v.low_stock_threshold
       HAVING COALESCE(SUM(i.quantity - i.reserved), 0) <= v.low_stock_threshold
     ) low"
  )
  .fetch_one(executor)
  .await?;
  Ok(count.unwrap_or(0))
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
        "SELECT COALESCE(SUM(total_cents), 0)::bigint FROM orders WHERE status IN ('paid', 'shipped', 'delivered')"
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

  let low_stock_count = count_low_stock_variants(pool).await?;

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
                                          <span class="text-muted-foreground">(crate::app::store::queries::format_datetime_br(order.created_at))</span>
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

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::count_low_stock_variants;

  async fn insert_variant(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    product_id: Uuid,
    threshold: i32,
  ) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color, low_stock_threshold)
       VALUES ($1, $2, $3, 'm', 'Preto', $4)",
    )
    .bind(id)
    .bind(product_id)
    .bind(format!("SKU-{}", id.simple()))
    .bind(threshold)
    .execute(&mut **tx)
    .await
    .expect("insert variant");
    id
  }

  async fn stock(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    variant_id: Uuid,
    warehouse_id: Uuid,
    quantity: i32,
    reserved: i32,
  ) {
    sqlx::query(
      "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
       VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(variant_id)
    .bind(warehouse_id)
    .bind(quantity)
    .bind(reserved)
    .execute(&mut **tx)
    .await
    .expect("insert inventory");
  }

  async fn insert_warehouse(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    active: bool,
  ) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO warehouses (id, code, name, active) VALUES ($1, $2, 'Teste', $3)",
    )
    .bind(id)
    .bind(format!("WH-{}", id.simple()))
    .bind(active)
    .execute(&mut **tx)
    .await
    .expect("insert warehouse");
    id
  }

  #[tokio::test]
  async fn low_stock_uses_per_variant_threshold_and_available_stock() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");
    // Freeze the snapshot so concurrent tests can't move the baseline.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
      .execute(&mut *tx)
      .await
      .expect("isolation");
    let baseline = count_low_stock_variants(&mut *tx).await.expect("count");

    let product_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'Teste', $2, 1000)",
    )
    .bind(product_id)
    .bind(format!("teste-{}", product_id.simple()))
    .execute(&mut *tx)
    .await
    .expect("insert product");
    let active = insert_warehouse(&mut tx, true).await;
    let second = insert_warehouse(&mut tx, true).await;
    let inactive = insert_warehouse(&mut tx, false).await;

    // 10 on hand, 6 reserved: 4 available <= threshold 5 -> low.
    let reserved_low = insert_variant(&mut tx, product_id, 5).await;
    stock(&mut tx, reserved_low, active, 10, 6).await;
    // 8 available, but this variant's threshold is 10 -> low.
    let high_threshold = insert_variant(&mut tx, product_id, 10).await;
    stock(&mut tx, high_threshold, active, 8, 0).await;
    // 3 available would be low under the old hard-coded 5; threshold 2 -> ok.
    let low_threshold = insert_variant(&mut tx, product_id, 2).await;
    stock(&mut tx, low_threshold, active, 3, 0).await;
    // Stock split across warehouses sums; inactive warehouses don't count.
    let split = insert_variant(&mut tx, product_id, 5).await;
    stock(&mut tx, split, active, 4, 0).await;
    stock(&mut tx, split, second, 4, 0).await;
    stock(&mut tx, split, inactive, 100, 0).await;
    // Only inactive-warehouse stock -> 0 available -> low.
    let only_inactive = insert_variant(&mut tx, product_id, 5).await;
    stock(&mut tx, only_inactive, inactive, 100, 0).await;

    let count = count_low_stock_variants(&mut *tx).await.expect("count");
    assert_eq!(count - baseline, 3);

    tx.rollback().await.expect("rollback");
  }
}
