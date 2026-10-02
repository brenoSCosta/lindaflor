use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{error::RouterErrorExt, page},
  view::{View, view},
};

use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::button_variants;
use crate::components::button::{ButtonSize, ButtonVariant};
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use lindaflor::auth::user::current_user_owned;

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
  let _su = current_user_owned(cx).await?.ok_or_redirect("/login")?;
  let pool = app_context::<PgPool>(cx);

  let orders = sqlx::query!(
        "SELECT id::text AS \"id!\", guest_email, status::text AS \"status!\", total_cents, created_at
         FROM orders
         ORDER BY created_at DESC
         LIMIT 20"
    )
    .fetch_all(pool)
    .await?;

  Ok(view! {
      <div class="mx-auto max-w-7xl px-4 py-8 md:px-8">
          <h1 class="text-2xl font-bold">"Meus Pedidos"</h1>
          <p class="mt-1 mb-8 text-sm text-muted-foreground">
              "Acompanhe o status dos seus pedidos."
          </p>

          if orders.is_empty() {
              <div class="rounded-xl border border-border bg-background p-12 text-center shadow-sm">
                  <p class="text-sm text-muted-foreground">"Você ainda não fez nenhum pedido."</p>
                  <a
                      href="/produtos"
                      class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                  >
                      "Explorar produtos"
                  </a>
              </div>
          } else {
              <div class="overflow-hidden rounded-xl border border-border bg-background shadow-sm">
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
                          for order in orders {
                              table_row(
                                  table_cell(
                                      <a
                                          href=(format!("/pedido/{}", order.id))
                                          class="font-mono text-primary"
                                      >
                                          (order.id.chars().take(8).collect::<String>())
                                      </a>
                                  )
                                  table_cell(badge(variant: status_variant(&order.status), (order.status)))
                                  table_cell("R$ " (order.total_cents / 100))
                                  table_cell(<span class="text-muted-foreground">(order.created_at.to_string())</span>)
                              )
                          }
                      )
                  )
              </div>
          }
      </div>
  })
}
