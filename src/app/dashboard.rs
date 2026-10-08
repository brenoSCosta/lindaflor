use sqlx::PgPool;
use time::PrimitiveDateTime;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page},
  view::{View, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_user;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::button_variants;
use crate::components::button::{ButtonSize, ButtonVariant};
use crate::components::container::container;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

pub struct RecentOrder {
  pub id: Uuid,
  pub status: String,
  pub total_cents: i32,
  pub created_at: PrimitiveDateTime,
}

pub async fn load_recent_orders(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Vec<RecentOrder>, sqlx::Error> {
  let rows = sqlx::query!(
    "SELECT id, status::text AS \"status!\", total_cents, created_at
         FROM orders
         WHERE user_id = $1
         ORDER BY created_at DESC
         LIMIT 5",
    user_id,
  )
  .fetch_all(pool)
  .await?;
  Ok(
    rows
      .into_iter()
      .map(|row| RecentOrder {
        id: row.id,
        status: row.status,
        total_cents: row.total_cents,
        created_at: row.created_at,
      })
      .collect(),
  )
}

fn status_variant(status: &str) -> BadgeVariant {
  match status {
    "paid" => BadgeVariant::Primary,
    "delivered" => BadgeVariant::Primary,
    "cancelled" => BadgeVariant::Destructive,
    _ => BadgeVariant::Secondary,
  }
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  // Authenticated page: anonymous visitors are redirected to `/login`
  // (see `require_user`) and never see order rows.
  let actor = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);

  let recent_orders = load_recent_orders(pool, actor.user.id).await?;

  Ok(view! {
      container(
          <div class="grid gap-4 @sm/page:grid-cols-2">
              <a
                  href=(href!(crate::app::conta::pedidos::page))
                  class="rounded-xl border border-border bg-background p-5 shadow-sm transition-colors hover:border-primary"
              >
                  <p class="text-sm text-muted-foreground">"Meus Pedidos"</p>
                  <p class="mt-1 text-2xl font-bold">"Ver pedidos"</p>
              </a>
              <a
                  href=(href!(crate::app::settings::page))
                  class="rounded-xl border border-border bg-background p-5 shadow-sm transition-colors hover:border-primary"
              >
                  <p class="text-sm text-muted-foreground">"Configurações"</p>
                  <p class="mt-1 text-2xl font-bold">"Ajustar conta"</p>
              </a>
          </div>

          <div class="overflow-hidden rounded-xl border border-border bg-background shadow-sm">
              <div class="flex items-center justify-between border-b border-border px-6 py-4">
                  <h2 class="font-semibold">"Pedidos recentes"</h2>
                  <a href=(href!(crate::app::conta::pedidos::page)) class="text-sm text-primary">"Ver todos"</a>
              </div>
              if recent_orders.is_empty() {
                  <div class="p-12 text-center">
                      <p class="text-sm text-muted-foreground">"Nenhum pedido ainda."</p>
                      <a
                          href=(href!(crate::app::produtos::page))
                          class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                      >
                          "Explorar produtos"
                      </a>
                  </div>
              } else {
                  table(
                      table_header(
                          table_row(
                              table_head("Pedido")
                              table_head("Status")
                              table_head("Total")
                              table_head("Data")
                          )
                      )
                      table_body(
                          for order in recent_orders {
                              table_row(
                                  table_cell(
                                      <a
                                          href=(href!(
                                              crate::app::pedido::id::page,
                                              crate::app::pedido::id::Id(order.id),
                                          ))
                                          class="font-mono text-primary"
                                      >
                                          (order.id.to_string().chars().take(8).collect::<String>())
                                      </a>
                                  )
                                  table_cell(badge(variant: status_variant(&order.status), (order.status)))
                                  table_cell("R$ " (order.total_cents / 100))
                                  table_cell((order.created_at.to_string()))
                              )
                          }
                      )
                  )
              }
          </div>
      )
  })
}
