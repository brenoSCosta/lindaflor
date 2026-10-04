use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{error::RouterErrorExt, href, page, query_params},
  runtime::{Event, shard, signal},
  view::{View, view},
};
use uuid::Uuid;

use crate::auth::user::current_user_owned;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::button_variants;
use crate::components::button::{ButtonSize, ButtonVariant};
use crate::components::container::container;
use crate::components::pagination::{
  pagination, pagination_content, pagination_item,
};
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

fn status_variant(status: &str) -> BadgeVariant {
  match status {
    "paid" => BadgeVariant::Primary,
    "delivered" => BadgeVariant::Primary,
    "cancelled" => BadgeVariant::Destructive,
    _ => BadgeVariant::Secondary,
  }
}

#[query_params(error = bad_request)]
struct MyOrdersQuery {
  page: Option<String>,
}

fn clamp_orders_page(page_num: i64) -> i64 {
  page_num.clamp(1, 10_000)
}

fn parse_page(raw: &Option<String>) -> i64 {
  raw
    .as_deref()
    .and_then(|value| value.parse().ok())
    .filter(|number| *number > 0)
    .unwrap_or(1)
}

/// Paginated order history for the signed-in user. Re-renders on the server
/// when the page signal changes, without reloading the page.
///
/// The shard endpoint does not run the page guard, so this re-checks
/// authentication itself and always scopes rows to the current user id.
/// Restored signal values are treated as user input.
#[shard("/conta/pedidos/lista")]
async fn my_orders(cx: &Cx, initial_page: i64) -> Result<impl View> {
  let su = current_user_owned(cx)
    .await?
    .ok_or_redirect(href!(crate::app::login::page).resolve(cx))?;
  let pool = app_context::<PgPool>(cx);
  let current_page = signal(cx, || clamp_orders_page(initial_page));

  let mut page_num = clamp_orders_page(current_page.get());
  let page_size: i64 = 20;

  let total: i64 = sqlx::query_scalar!(
    "SELECT COUNT(*) FROM orders WHERE user_id = $1",
    su.user.id
  )
  .fetch_one(pool)
  .await?
  .unwrap_or(0);
  let total_pages = if total == 0 {
    0
  } else {
    (total + page_size - 1) / page_size
  };
  if total_pages > 0 && page_num > total_pages {
    page_num = total_pages;
  }

  let orders = sqlx::query!(
        "SELECT id::text AS \"id!\", guest_email, status::text AS \"status!\", total_cents, created_at
         FROM orders
         WHERE user_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
        su.user.id,
        page_size,
        (page_num - 1) * page_size,
    )
    .fetch_all(pool)
    .await?;

  let is_empty = orders.is_empty();
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
      if is_empty {
          <div class="rounded-xl border border-border bg-background p-12 text-center shadow-sm">
              <p class="text-sm text-muted-foreground">"Você ainda não fez nenhum pedido."</p>
              <a
                  href=(href!(crate::app::produtos::page))
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
                      #[key(order.id.clone())]
                      for order in orders {
                          let order_uuid = Uuid::parse_str(&order.id)
                            .unwrap_or_else(|_| Uuid::nil());
                          table_row(
                              table_cell(
                                  <a
                                      href=(href!(crate::app::pedido::id::page, crate::app::pedido::id::Id(order_uuid)))
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

          if show_pagination {
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
          }
      }
  })
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _su = current_user_owned(cx)
    .await?
    .ok_or_redirect(href!(crate::app::login::page).resolve(cx))?;
  let query = query_params::<MyOrdersQuery>(cx)?;
  let page_num = clamp_orders_page(parse_page(&query.page));

  Ok(view! {
      container(
          <h1 class="text-2xl font-bold">"Meus Pedidos"</h1>
          <p class="text-sm text-muted-foreground">
              "Acompanhe o status dos seus pedidos."
          </p>

          my_orders(initial_page: page_num)
      )
  })
}
