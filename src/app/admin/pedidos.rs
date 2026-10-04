pub mod id;

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
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::container::container;
use crate::components::input::input;
use crate::components::pagination::{
  pagination, pagination_content, pagination_item,
};
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

#[query_params(error = bad_request)]
struct OrdersQuery {
  q: Option<String>,
  status: Option<String>,
  page: Option<String>,
}

const ORDER_QUERY_MAX: usize = 80;

fn clamp_order_query(raw: &str) -> String {
  raw.trim().chars().take(ORDER_QUERY_MAX).collect()
}

fn clamp_order_status(raw: &str) -> String {
  match raw.trim() {
    "pending_payment" | "paid" | "cancelled" | "delivered" => {
      raw.trim().to_string()
    }
    _ => "all".to_string(),
  }
}

fn clamp_order_page(page_num: i64) -> i64 {
  page_num.clamp(1, 10_000)
}

fn parse_page(raw: &Option<String>) -> i64 {
  raw
    .as_deref()
    .and_then(|value| value.parse().ok())
    .filter(|number| *number > 0)
    .unwrap_or(1)
}

async fn require_admin(cx: &Cx) -> Result<SessionUser> {
  let su = require_user(cx).await?;
  if !service::is_admin(su.user.role.as_deref()) {
    return Err(see_other_fallback(cx).into());
  }
  Ok(su)
}

fn see_other_fallback(cx: &Cx) -> topcoat::router::error::SeeOther {
  topcoat::router::error::see_other(
    href!(crate::app::dashboard::page).resolve(cx),
  )
}

/// Search, status filter, and pagination. Re-renders on the server when any
/// signal changes, without reloading the page.
///
/// The shard endpoint does not run the page guard, so this checks the admin
/// role itself. Restored signal values are treated as user input.
#[shard("/admin/pedidos/lista")]
async fn order_directory(
  cx: &Cx,
  q: String,
  status: String,
  initial_page: i64,
) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let q = signal(cx, || clamp_order_query(&q));
  let status = signal(cx, || clamp_order_status(&status));
  let current_page = signal(cx, || clamp_order_page(initial_page));

  let query = clamp_order_query(&q.get());
  let active_status = clamp_order_status(&status.get());
  let mut page_num = clamp_order_page(current_page.get());
  let page_size: i64 = 20;

  let total: i64 = sqlx::query_scalar!(
    "SELECT COUNT(*) FROM orders o WHERE ($1 = '' OR o.guest_email ILIKE '%' || $1 || '%') AND ($2 = 'all' OR o.status::text = $2)",
    query,
    active_status,
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
        "SELECT o.id::text AS \"id!\", o.guest_email, o.status::text AS status, o.total_cents, o.created_at,
                (SELECT COUNT(*) FROM order_items oi WHERE oi.order_id = o.id) as item_count
         FROM orders o
         WHERE ($1 = '' OR o.guest_email ILIKE '%' || $1 || '%')
           AND ($2 = 'all' OR o.status::text = $2)
         ORDER BY o.created_at DESC
         LIMIT $3 OFFSET $4",
        query,
        active_status,
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

  const STATUSES: [(&str, &str); 5] = [
    ("all", "Todos"),
    ("pending_payment", "Pendentes"),
    ("paid", "Pagos"),
    ("delivered", "Entregues"),
    ("cancelled", "Cancelados"),
  ];

  Ok(view! {

          input(attrs: attributes! {
              id="order-search"
              type="search"
              placeholder="Buscar por e-mail"
              aria-label="Buscar pedidos"
              :value=$(q.get())
              @input=$(|e: Event| {
                  q.set(e.target.value);
                  current_page.set(1i64);
              })
          })

      <div class="flex flex-wrap items-center gap-2">
          for (key, label) in STATUSES {
              let key_owned = key.to_string();
              let selected = active_status == key;
              <button
                  type="button"
                  class=(button_variants(
                      if selected {
                          ButtonVariant::Primary
                      } else {
                          ButtonVariant::Outline
                      },
                      ButtonSize::Sm,
                  ))
                  @click=$(|_e: Event| {
                      status.set(key_owned.to_owned());
                      current_page.set(1i64);
                  })
              >
                  (label)
              </button>
          }
      </div>

      if is_empty {
          <p class="text-sm text-muted-foreground">"Nenhum pedido encontrado."</p>
      } else {
          table(
              table_header(
                  table_row(
                      table_head("ID")
                      table_head("Cliente")
                      table_head("Status")
                      table_head("Itens")
                      table_head("Total")
                      table_head("Data")
                  )
              )
              table_body(
                  #[key(order.id.clone())]
                  for order in orders {
                      table_row(
                          table_cell(
                              <a href=(href!(crate::app::admin::pedidos::id::page, crate::app::admin::pedidos::id::Id(order.id.clone()))) class="font-mono text-sm text-primary hover:underline">(order.id)</a>
                          )
                          table_cell((order.guest_email))
                          table_cell((order.status))
                          table_cell((order.item_count))
                          table_cell("R$ " (order.total_cents / 100))
                          table_cell(
                              <span class="text-muted-foreground">(order.created_at.to_string())</span>
                          )
                      )
                  }
              )
          )
      }

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
  })
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let query = query_params::<OrdersQuery>(cx)?;
  let q = clamp_order_query(query.q.as_deref().unwrap_or(""));
  let status = clamp_order_status(query.status.as_deref().unwrap_or("all"));
  let page_num = clamp_order_page(parse_page(&query.page));

  Ok(view! {
      container(
          <h1 class="text-2xl font-semibold tracking-tight">"Pedidos"</h1>
          <p class="text-muted-foreground">"Acompanhe pedidos da loja e status de pagamento."</p>

          order_directory(q: q, status: status, initial_page: page_num)
      )
  })
}
