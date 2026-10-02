use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{error::RouterErrorExt, page},
  view::{View, view},
};

use crate::components::avatar::{AvatarSize, avatar, avatar_fallback};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::button_variants;
use crate::components::button::{ButtonSize, ButtonVariant};
use crate::components::card::{
  card, card_content, card_description, card_header, card_title,
};
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

fn initials(name: &str) -> String {
  name
    .split_whitespace()
    .filter_map(|w| w.chars().next())
    .take(2)
    .collect::<String>()
    .to_uppercase()
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let su = current_user_owned(cx).await?.ok_or_redirect("/login")?;
  let pool = app_context::<PgPool>(cx);

  let user_name = su.user.name.clone();
  let user_email = su.user.email.clone();
  let user_role = su.user.role.clone().unwrap_or_else(|| "user".to_string());
  let user_initials = initials(&user_name);

  let recent_orders = sqlx::query!(
        "SELECT id::text AS \"id!\", guest_email, status::text AS \"status!\", total_cents, created_at
         FROM orders
         ORDER BY created_at DESC
         LIMIT 5"
    )
    .fetch_all(pool)
    .await?;

  Ok(view! {
      <div class="mx-auto max-w-7xl px-4 py-8 md:px-8">
          <h1 class="text-2xl font-bold">"Painel"</h1>
          <p class="mt-1 mb-8 text-sm text-muted-foreground">
              "Bem-vinda de volta, "
              (user_name.as_str())
              ". Gerencie seus pedidos e configurações."
          </p>

          card(
              card_header(
                  card_title((user_name.clone()))
                  card_description((user_email.clone()))
              )
              card_content(
                  <div class="flex items-center gap-4">
                      avatar(
                          size: AvatarSize::Lg,
                          avatar_fallback((user_initials))
                      )
                      <div class="mt-1">
                          badge(variant: BadgeVariant::Secondary, (user_role))
                      </div>
                  </div>
              )
          )

          <div class="my-6 grid gap-4 sm:grid-cols-3">
              <a
                  href="/conta/pedidos"
                  class="rounded-xl border border-border bg-background p-5 shadow-sm transition-colors hover:border-primary"
              >
                  <p class="text-sm text-muted-foreground">"Meus Pedidos"</p>
                  <p class="mt-1 text-2xl font-bold">"Ver pedidos"</p>
              </a>
              <a
                  href="/settings"
                  class="rounded-xl border border-border bg-background p-5 shadow-sm transition-colors hover:border-primary"
              >
                  <p class="text-sm text-muted-foreground">"Configurações"</p>
                  <p class="mt-1 text-2xl font-bold">"Ajustar conta"</p>
              </a>
              <a
                  href="/permissions"
                  class="rounded-xl border border-border bg-background p-5 shadow-sm transition-colors hover:border-primary"
              >
                  <p class="text-sm text-muted-foreground">"Permissões"</p>
                  <p class="mt-1 text-2xl font-bold">"Ver acesso"</p>
              </a>
          </div>

          <div class="overflow-hidden rounded-xl border border-border bg-background shadow-sm">
              <div class="flex items-center justify-between border-b border-border px-6 py-4">
                  <h2 class="font-semibold">"Pedidos recentes"</h2>
                  <a href="/conta/pedidos" class="text-sm text-primary">"Ver todos"</a>
              </div>
              if recent_orders.is_empty() {
                  <div class="p-12 text-center">
                      <p class="text-sm text-muted-foreground">"Nenhum pedido ainda."</p>
                      <a
                          href="/produtos"
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
                                          href=(format!("/pedido/{}", order.id))
                                          class="font-mono text-primary"
                                      >
                                          (order.id.chars().take(8).collect::<String>())
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
      </div>
  })
}
