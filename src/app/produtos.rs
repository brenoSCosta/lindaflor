pub mod slug;

use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page, query_params},
  runtime::{Event, shard, signal},
  view::{View, attributes, view},
};

use crate::app::store::catalog_filters::{
  catalog_toolbar, repeated_query_values,
};
use crate::app::store::product_card::product_card;
use crate::app::store::queries::{
  CatalogFilters, STOREFRONT_PAGE_SIZE, clamp_catalog_page,
  count_products_with_filters, list_filter_colors,
  list_products_with_filters,
};
use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::container::container;
use crate::components::input::input;
use crate::components::pagination::{
  pagination, pagination_content, pagination_item,
};

#[query_params(error = bad_request)]
struct ProductsQuery {
  category: Option<String>,
  search: Option<String>,
  destaque: Option<bool>,
  esgotados: Option<bool>,
  page: Option<i64>,
  sort: Option<String>,
  // `tamanho` / `cor` repeat (one per checkbox); query_params can't read
  // sequences, so they come from `repeated_query_values` instead.
  preco_min: Option<String>,
  preco_max: Option<String>,
  disponivel: Option<bool>,
  sale: Option<bool>,
}

const CATEGORIES: [(&str, &str); 7] = [
  ("all", "Todos"),
  ("biquini", "Biquínis"),
  ("biquini_top", "Tops"),
  ("biquini_bottom", "Cantinhas"),
  ("maio", "Maiôs"),
  ("saida_praia", "Saídas"),
  ("acessorio", "Acessórios"),
];

const SEARCH_MAX: usize = 80;

fn catalog_category(raw: &str) -> String {
  match raw.trim() {
    "biquini" | "biquini_top" | "biquini_bottom" | "maio" | "saida_praia"
    | "acessorio" => raw.trim().to_string(),
    _ => "all".to_string(),
  }
}

const SIZES: [&str; 5] = ["pp", "p", "m", "g", "gg"];
const COLOR_FILTER_MAX: usize = 20;
const COLOR_NAME_MAX: usize = 40;

/// Reais in the query string ("89,90" or "89.90") to cents; junk is ignored.
fn price_param_cents(raw: Option<&str>) -> Option<i64> {
  let value: f64 = raw?.trim().replace(',', ".").parse().ok()?;
  (0.0..=100_000.0)
    .contains(&value)
    .then(|| (value * 100.0).round() as i64)
}

/// Normalizes the facet params, which are request input like category/search.
pub(crate) fn catalog_filters(
  sizes: Vec<String>,
  colors: Vec<String>,
  preco_min: Option<&str>,
  preco_max: Option<&str>,
  available_only: bool,
  sale_only: bool,
) -> CatalogFilters {
  let mut sizes: Vec<String> = sizes
    .into_iter()
    .map(|size| size.trim().to_lowercase())
    .filter(|size| SIZES.contains(&size.as_str()))
    .collect();
  sizes.dedup();
  let colors = colors
    .into_iter()
    .map(|color| {
      color
        .trim()
        .chars()
        .take(COLOR_NAME_MAX)
        .collect::<String>()
    })
    .filter(|color| !color.is_empty())
    .take(COLOR_FILTER_MAX)
    .collect();
  CatalogFilters {
    sizes,
    colors,
    price_min_cents: price_param_cents(preco_min),
    price_max_cents: price_param_cents(preco_max),
    available_only,
    sale_only,
  }
}

fn catalog_search(raw: &str) -> String {
  raw.trim().chars().take(SEARCH_MAX).collect()
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let query = query_params::<ProductsQuery>(cx)?;
  let category = catalog_category(query.category.as_deref().unwrap_or("all"));
  let search = catalog_search(query.search.as_deref().unwrap_or(""));
  let featured_only = query.destaque.unwrap_or(false);
  let include_out_of_stock = query.esgotados.unwrap_or(false);
  let page = clamp_catalog_page(query.page.unwrap_or(1));
  let sort = query.sort.clone().unwrap_or_else(|| "destaque".to_string());
  let tamanho = repeated_query_values(cx, "tamanho");
  let cor = repeated_query_values(cx, "cor");
  let preco_min = query.preco_min.as_deref();
  let preco_max = query.preco_max.as_deref();
  let disponivel = query.disponivel.unwrap_or(false);
  let sale = query.sale.unwrap_or(false);

  Ok(view! {
      catalog_results(
          category: category,
          search: search,
          featured_only: featured_only,
          include_out_of_stock: include_out_of_stock,
          catalog_page: page,
          sort: sort,
          tamanho: tamanho,
          cor: cor,
          preco_min: preco_min.map(|s| s.to_string()),
          preco_max: preco_max.map(|s| s.to_string()),
          disponivel: disponivel,
          sale: sale,
      )
  })
}

/// Category filters and search results. Re-renders on the server when either
/// signal changes, without reloading the rest of the storefront.
///
/// Arguments and restored signals are request input: categories outside the
/// catalog and oversized searches are normalized before the query runs.
#[shard("/busca/produtos")]
async fn catalog_results(
  cx: &Cx,
  category: String,
  search: String,
  featured_only: bool,
  include_out_of_stock: bool,
  catalog_page: i64,
  sort: String,
  tamanho: Vec<String>,
  cor: Vec<String>,
  preco_min: Option<String>,
  preco_max: Option<String>,
  disponivel: bool,
  sale: bool,
) -> Result<impl View> {
  let category = signal(cx, || catalog_category(&category));
  let search = signal(cx, || catalog_search(&search));
  let sort_signal = signal(cx, || sort.clone());
  // String flags, not bools: a captured bool becomes a truthy JS object inside raw!.
  let featured_flag = signal(cx, || {
    if featured_only {
      "true".to_owned()
    } else {
      String::new()
    }
  });
  let sold_out_flag = signal(cx, || {
    if include_out_of_stock {
      "true".to_owned()
    } else {
      String::new()
    }
  });
  let current_page = signal(cx, || clamp_catalog_page(catalog_page));
  let active = catalog_category(&category.get());
  let term = catalog_search(&search.get());

  let pool = app_context::<PgPool>(cx);
  let category_filter = (active != "all").then_some(active.as_str());
  let search_filter = (!term.is_empty()).then_some(term.as_str());

  let filters = catalog_filters(
    tamanho.clone(),
    cor.clone(),
    preco_min.as_deref(),
    preco_max.as_deref(),
    disponivel,
    sale,
  );
  let include_out_of_stock = include_out_of_stock && !filters.available_only;

  let total = count_products_with_filters(
    pool,
    category_filter,
    search_filter,
    featured_only,
    include_out_of_stock,
    &filters,
  )
  .await?;
  let page_size = STOREFRONT_PAGE_SIZE;
  let total_pages = if total == 0 {
    0
  } else {
    (total + page_size - 1) / page_size
  };
  let mut page_num = clamp_catalog_page(current_page.get());
  if total_pages > 0 && page_num > total_pages {
    page_num = total_pages;
  }
  let offset = (page_num - 1) * page_size;
  let sort_str = sort_signal.get();
  let products = list_products_with_filters(
    pool,
    category_filter,
    search_filter,
    featured_only,
    include_out_of_stock,
    &filters,
    &sort_str,
    page_size,
    offset,
  )
  .await?;
  let store = crate::app::utils::object_store(cx);
  let products =
    crate::app::admin::produtos::resolve_summary_images(&store, products).await;

  let available_colors =
    list_filter_colors(pool, category_filter).await?;

  // Update heading for special cases
  let heading = if sale {
    "Liquida".to_string()
  } else if sort_str == "novos" {
    "Novidades".to_string()
  } else if term.is_empty() {
    "Catálogo".to_string()
  } else {
    format!("Busca: {term}")
  };

  // Prepare toolbar parameters (clone before view! macro to avoid ownership issues)
  let toolbar_category = active.to_string();
  let toolbar_search = term.to_string();
  let toolbar_sort = sort_str.clone();
  let toolbar_tamanho = tamanho.clone();
  let toolbar_cor = cor.clone();
  let toolbar_preco_min = preco_min.map(|s| s.to_string());
  let toolbar_preco_max = preco_max.map(|s| s.to_string());
  let toolbar_colors = available_colors.clone();

  let _count = total;
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

  Ok(view! {
      <section class="border-b border-border">
          container(
              breadcrumb(
                  breadcrumb_list(
                      breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::page)) }, "Início"))
                      breadcrumb_separator()
                      breadcrumb_item(breadcrumb_page("Catálogo"))
                  )
              )
              <h1 class="mt-4 text-4xl font-bold tracking-tight">(heading)</h1>
              <p class="mt-3 max-w-2xl text-muted-foreground">
                  "Biquínis, maiôs e saídas de praia, peças selecionadas para o seu verão."
              </p>
          )
      </section>

      container(
          <div class="flex flex-wrap items-center gap-2">
              for (key, label) in CATEGORIES {
                  let key_owned = key.to_string();
                  let selected = active == key;
                  <button
                      id=(format!("catalog-category-{key}"))
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
                          category.set(key_owned.to_owned());
                          current_page.set(1i64);
                          let category = category.get();
                          let search = search.get();
                          let featured_flag = featured_flag.get();
                          let sold_out_flag = sold_out_flag.get();
                          raw!(
                              r#"const params = new URLSearchParams();
                                 const category = String(${category});
                                 const term = String(${search}).trim();
                                 if (category !== "all") params.set("category", category);
                                 if (term) params.set("search", term);
                                 if (String(${featured_flag}) === "true") params.set("destaque", "true");
                                 if (String(${sold_out_flag}) === "true") params.set("esgotados", "true");
                                 const qs = params.toString();
                                 history.replaceState(null, "", qs ? "/produtos?" + qs : "/produtos");"#,
                              {
                                  let _ = (category, search, featured_flag, sold_out_flag);
                              }
                          );
                      })
                  >
                      (label)
                  </button>
              }
          </div>

              input(attrs: attributes! {
                  id="catalog-search"
                  type="search"
                  placeholder="Buscar produtos..."
                  aria-label="Buscar produtos"
                  :value=$(search.get())
                  @input=$(|e: Event| {
                      search.set(e.target.value);
                      current_page.set(1i64);
                      let category = category.get();
                      let search = search.get();
                      let featured_flag = featured_flag.get();
                      let sold_out_flag = sold_out_flag.get();
                      raw!(
                          r#"const params = new URLSearchParams();
                             const category = String(${category});
                             const term = String(${search}).trim();
                             if (category !== "all") params.set("category", category);
                             if (term) params.set("search", term);
                             if (String(${featured_flag}) === "true") params.set("destaque", "true");
                             if (String(${sold_out_flag}) === "true") params.set("esgotados", "true");
                             const qs = params.toString();
                             history.replaceState(null, "", qs ? "/produtos?" + qs : "/produtos");"#,
                          {
                              let _ = (category, search, featured_flag, sold_out_flag);
                          }
                      );
                  })
              })

          // Filter toolbar
          catalog_toolbar(
              category: toolbar_category,
              search: toolbar_search,
              sort: toolbar_sort,
              tamanho: toolbar_tamanho,
              cor: toolbar_cor,
              preco_min: toolbar_preco_min,
              preco_max: toolbar_preco_max,
              disponivel: disponivel,
              sale: sale,
              available_colors: toolbar_colors,
          )

          if is_empty {
              <p class="text-muted-foreground">
                  "Nenhum produto encontrado. "
                  <a
                      href="/produtos"
                      class="text-primary underline"
                  >
                      "Limpar filtros"
                  </a>
              </p>
          } else {
              <div class="grid gap-8 @sm/page:grid-cols-2 @2xl/page:grid-cols-3">
                  #[key(product.id.to_string())]
                  for product in products {
                      product_card(product: product)
                  }
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
      )
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn price_param_accepts_comma_and_rejects_junk() {
    assert_eq!(price_param_cents(Some("89,90")), Some(8990));
    assert_eq!(price_param_cents(Some("100")), Some(10000));
    assert_eq!(price_param_cents(Some("-1")), None);
    assert_eq!(price_param_cents(Some("NaN")), None);
    assert_eq!(price_param_cents(Some("abc")), None);
    assert_eq!(price_param_cents(None), None);
  }

  #[test]
  fn catalog_filters_drop_unknown_sizes_and_cap_colors() {
    let colors = (0..30).map(|i| format!("cor{i}")).collect();
    let filters = catalog_filters(
      vec!["P".into(), "xxl".into(), "m".into()],
      colors,
      None,
      None,
      false,
      true,
    );
    assert_eq!(filters.sizes, vec!["p".to_string(), "m".to_string()]);
    assert_eq!(filters.colors.len(), COLOR_FILTER_MAX);
    assert!(filters.sale_only);
  }
}
