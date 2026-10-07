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

use crate::app::store::queries::{category_label, format_price, list_products};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{card, card_footer, card_header, card_title};
use crate::components::container::container;
use crate::components::input::input;

#[query_params(error = bad_request)]
struct ProductsQuery {
  category: Option<String>,
  search: Option<String>,
  destaque: Option<bool>,
  esgotados: Option<bool>,
}

const CATEGORIES: [(&str, &str); 5] = [
  ("all", "Todos"),
  ("biquini", "Biquínis"),
  ("maio", "Maiôs"),
  ("saida_praia", "Saídas"),
  ("acessorio", "Acessórios"),
];

const SEARCH_MAX: usize = 80;

fn catalog_category(raw: &str) -> String {
  match raw.trim() {
    "biquini" | "maio" | "saida_praia" | "acessorio" => raw.trim().to_string(),
    _ => "all".to_string(),
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

  Ok(view! {
      catalog_results(
          category: category,
          search: search,
          featured_only: featured_only,
          include_out_of_stock: include_out_of_stock,
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
) -> Result<impl View> {
  let category = signal(cx, || catalog_category(&category));
  let search = signal(cx, || catalog_search(&search));
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
  let active = catalog_category(&category.get());
  let term = catalog_search(&search.get());

  let pool = app_context::<PgPool>(cx);
  let category_filter = (active != "all").then_some(active.as_str());
  let search_filter = (!term.is_empty()).then_some(term.as_str());
  let products = list_products(
    pool,
    category_filter,
    search_filter,
    featured_only,
    include_out_of_stock,
    None,
  )
  .await?;
  let store = crate::app::utils::object_store(cx);
  let products =
    crate::app::admin::produtos::resolve_summary_images(&store, products).await;

  let heading = if term.is_empty() {
    "Catálogo".to_string()
  } else {
    format!("Busca: {term}")
  };
  let count = products.len();
  let is_empty = products.is_empty();

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

          if is_empty {
              <p class="text-muted-foreground">
                  "Nenhum produto encontrado. "
                  <button
                      type="button"
                      class="text-primary underline"
                      @click=$(|_e: Event| {
                          category.set("all".to_owned());
                          search.set("".to_owned());
                          let featured_flag = featured_flag.get();
                          let sold_out_flag = sold_out_flag.get();
                          raw!(
                              r#"const params = new URLSearchParams();
                                 if (String(${featured_flag}) === "true") params.set("destaque", "true");
                                 if (String(${sold_out_flag}) === "true") params.set("esgotados", "true");
                                 const qs = params.toString();
                                 history.replaceState(null, "", qs ? "/produtos?" + qs : "/produtos");"#,
                              {
                                  let _ = (featured_flag, sold_out_flag);
                              }
                          );
                      })
                  >
                      "Limpar filtros"
                  </button>
              </p>
          } else {
              <p class="text-sm text-muted-foreground">(count) " peças"</p>
              <div class="grid gap-4 @sm/page:grid-cols-2 @2xl/page:grid-cols-3">
                  #[key(product.id.to_string())]
                  for product in products {
                      let image_url = product
                          .image_url
                          .clone()
                          .unwrap_or_else(|| "/static/product-fallback.svg".to_string());
                      let name = product.name.clone();
                      let product_url = href!(
                        slug::page,
                        slug::Slug(product.slug.clone())
                      );
                      let category_text = category_label(&product.category).to_string();
                      let price_text = format_price(product.price_in_cents);
                      let sold_out = product.available_total == 0;

                      card(
                          attrs: attributes! { class="group overflow-hidden" },
                          <a href=(product_url) class="block">
                              <div class="aspect-[4/5] overflow-hidden">
                                  <img
                                      src=(image_url)
                                      alt=(name.clone())
                                      class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
                                  >
                              </div>
                          </a>
                          card_header(
                              card_title((name))
                              badge(variant: BadgeVariant::Secondary, (category_text))
                          )
                          card_footer(
                              <span class="text-sm font-medium">(price_text)</span>
                              if sold_out {
                                  badge(variant: BadgeVariant::Destructive, "Esgotado")
                              }
                          )
                      )
                  }
              </div>
          }
      )
  })
}
