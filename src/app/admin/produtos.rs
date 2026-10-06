pub mod id;
pub mod novo;

use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page, query_params},
  runtime::{Event, shard, signal},
  view::{View, attributes, view},
};

use crate::app::auth_helpers::require_admin;
use crate::components::badge::{BadgeVariant, badge};
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
struct ProductsAdminQuery {
  q: Option<String>,
  active: Option<String>,
  page: Option<String>,
}

const PRODUCT_QUERY_MAX: usize = 80;

fn clamp_product_query(raw: &str) -> String {
  raw.trim().chars().take(PRODUCT_QUERY_MAX).collect()
}

fn clamp_product_active(raw: &str) -> String {
  match raw.trim() {
    "true" | "false" => raw.trim().to_string(),
    _ => "all".to_string(),
  }
}

fn clamp_product_page(page_num: i64) -> i64 {
  page_num.clamp(1, 10_000)
}

fn parse_page(raw: &Option<String>) -> i64 {
  raw
    .as_deref()
    .and_then(|value| value.parse().ok())
    .filter(|number| *number > 0)
    .unwrap_or(1)
}

fn category_labels(cat: &str) -> &str {
  match cat {
    "biquini" => "Biquíni",
    "maio" => "Maiô",
    "saida_praia" => "Saída de Praia",
    "acessorio" => "Acessório",
    _ => cat,
  }
}

/// Search, active filter, and pagination. Re-renders on the server when any
/// signal changes, without reloading the page.
///
/// The shard endpoint does not run the page guard, so this checks the admin
/// role itself. Restored signal values are treated as user input.
#[shard("/admin/produtos/lista")]
async fn product_directory(
  cx: &Cx,
  q: String,
  active: String,
  initial_page: i64,
) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let q = signal(cx, || clamp_product_query(&q));
  let active = signal(cx, || clamp_product_active(&active));
  let current_page = signal(cx, || clamp_product_page(initial_page));

  let query = clamp_product_query(&q.get());
  let active_filter = clamp_product_active(&active.get());
  let mut page_num = clamp_product_page(current_page.get());
  let page_size: i64 = 20;

  let total: i64 = sqlx::query_scalar!(
    "SELECT COUNT(*) FROM products p WHERE ($1 = '' OR p.name ILIKE '%' || $1 || '%') AND ($2 = 'all' OR p.active = ($2 = 'true'))",
    query,
    active_filter,
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

  let products = sqlx::query!(
        "SELECT p.id::text AS \"id!\", p.name, p.slug, p.category::text AS \"category!\", p.price_in_cents, p.active,
                COUNT(DISTINCT pv.id) as variant_count,
                COALESCE(SUM(i.quantity), 0) as available_total
         FROM products p
         LEFT JOIN product_variants pv ON pv.product_id = p.id
         LEFT JOIN inventory i ON i.variant_id = pv.id
         WHERE ($1 = '' OR p.name ILIKE '%' || $1 || '%')
           AND ($2 = 'all' OR p.active = ($2 = 'true'))
         GROUP BY p.id
         ORDER BY p.name
         LIMIT $3 OFFSET $4",
        query,
        active_filter,
        page_size,
        (page_num - 1) * page_size,
    )
    .fetch_all(pool)
    .await?;

  let is_empty = products.is_empty();
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

  const FILTERS: [(&str, &str); 3] =
    [("all", "Todos"), ("true", "Ativos"), ("false", "Inativos")];

  Ok(view! {
          input(attrs: attributes! {
              id="product-search"
              type="search"
              placeholder="Buscar por nome"
              aria-label="Buscar produtos"
              :value=$(q.get())
              @input=$(|e: Event| {
                  q.set(e.target.value);
                  current_page.set(1i64);
              })
          })
      <div class="flex flex-wrap items-center gap-2">
          for (key, label) in FILTERS {
              let key_owned = key.to_string();
              let selected = active_filter == key;
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
                      active.set(key_owned.to_owned());
                      current_page.set(1i64);
                  })
              >
                  (label)
              </button>
          }
      </div>

      if is_empty {
          <p class="text-sm text-muted-foreground">"Nenhum produto encontrado."</p>
      } else {
          table(
              table_header(
                  table_row(
                      table_head("Nome")
                      table_head("Categoria")
                      table_head("Preço")
                      table_head("Variantes")
                      table_head("Disponível")
                      table_head("Status")
                  )
              )
              table_body(
                  #[key(product.id.clone())]
                  for product in products {
                      table_row(
                          table_cell(
                              <a href=(href!(crate::app::admin::produtos::id::page, crate::app::admin::produtos::id::Id(product.id.clone()))) class="font-medium text-primary hover:underline">
                                  (product.name)
                              </a>
                          )
                          table_cell((category_labels(&product.category)))
                          table_cell("R$ " (product.price_in_cents / 100))
                          table_cell((product.variant_count.unwrap_or(0)))
                          table_cell((product.available_total.unwrap_or(0)))
                          table_cell(
                              if product.active {
                                  badge(variant: BadgeVariant::Primary, "Ativo")
                              } else {
                                  badge(variant: BadgeVariant::Secondary, "Inativo")
                              }
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
  let query = query_params::<ProductsAdminQuery>(cx)?;
  let q = clamp_product_query(query.q.as_deref().unwrap_or(""));
  let active = clamp_product_active(query.active.as_deref().unwrap_or("all"));
  let page_num = clamp_product_page(parse_page(&query.page));

  Ok(view! {
      container(
          <div class="flex flex-wrap items-center justify-between gap-4">
              <div>
                  <h1 class="text-2xl font-semibold tracking-tight">"Produtos"</h1>
                  <p class="text-muted-foreground">"Lista de produtos cadastrados no catálogo."</p>
              </div>
              <a
                  href=(href!(crate::app::admin::produtos::novo::page))
                  class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
              >
                  "Novo produto"
              </a>
          </div>

          product_directory(q: q, active: active, initial_page: page_num)
      )
  })
}
