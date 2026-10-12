use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{error::bad_request, href, page, path_param, query_params},
  view::{View, ViewExt, attributes, class, view},
};

use crate::app::store::catalog_filters::{
  catalog_toolbar, repeated_query_values,
};
use crate::app::store::queries::{
  STOREFRONT_PAGE_SIZE, category_label, clamp_catalog_page,
  count_collection_products, count_products_with_filters, format_price,
  get_collection_by_slug, list_filter_colors, list_products_with_filters,
};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{card, card_footer, card_header, card_title};
use crate::components::container::container;
use crate::components::pagination::{
  pagination, pagination_content, pagination_item,
};

path_param!(pub(crate) slug);

#[query_params(error = bad_request)]
struct CollectionQuery {
  page: Option<i64>,
  sort: Option<String>,
  // `tamanho` / `cor` repeat (one per checkbox); query_params can't read
  // sequences, so they come from `repeated_query_values` instead.
  preco_min: Option<String>,
  preco_max: Option<String>,
  disponivel: Option<bool>,
  sale: Option<bool>,
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let slug = path_param::<Slug>(cx);

  let collection = match get_collection_by_slug(pool, slug).await? {
        Some(c) => c,
        None => {
            return Ok(view! {
                container(
                    <h1 class="text-2xl font-bold">"Coleção não encontrada"</h1>
                    <a
                        href=(href!(crate::app::colecoes::page))
                        class=(class!(button_variants(ButtonVariant::Primary, ButtonSize::Md), "mx-auto mt-6 w-fit"))
                    >
                        "Ver coleções"
                    </a>
                )
            }.boxed())
        }
    };

  let product_count = count_collection_products(pool, &collection.id).await?;
  let query = query_params::<CollectionQuery>(cx)
    .map_err(|err| bad_request(format!("consulta inválida: {err}")))?;

  let sort = query.sort.clone().unwrap_or_else(|| "destaque".to_string());
  let tamanho = repeated_query_values(cx, "tamanho");
  let cor = repeated_query_values(cx, "cor");
  let preco_min = query.preco_min.as_deref();
  let preco_max = query.preco_max.as_deref();
  let disponivel = query.disponivel.unwrap_or(false);
  let sale = query.sale.unwrap_or(false);

  let filters = crate::app::produtos::catalog_filters(
    tamanho.clone(),
    cor.clone(),
    preco_min,
    preco_max,
    disponivel,
    sale,
  );
  // Collections list sold-out pieces too, unless the shopper asks otherwise.
  let include_out_of_stock = !filters.available_only;
  let available_colors = list_filter_colors(pool, None, Some(slug)).await?;

  let mut page_num = clamp_catalog_page(query.page.unwrap_or(1));
  let page_size = STOREFRONT_PAGE_SIZE;
  let total = count_products_with_filters(
    pool,
    None,
    None,
    false,
    include_out_of_stock,
    Some(slug),
    &filters,
  )
  .await?;
  let total_pages = if total == 0 {
    0
  } else {
    (total + page_size - 1) / page_size
  };
  if total_pages > 0 && page_num > total_pages {
    page_num = total_pages;
  }
  let products = list_products_with_filters(
    pool,
    None,
    None,
    false,
    include_out_of_stock,
    Some(slug),
    &filters,
    &sort,
    page_size,
    (page_num - 1) * page_size,
  )
  .await?;
  let show_pagination = total_pages > 1;
  let has_prev = page_num > 1;
  let has_next = total_pages > 0 && page_num < total_pages;
  let page_label = if total_pages == 0 {
    "Página 1 de 1".to_string()
  } else {
    format!("Página {page_num} de {total_pages}")
  };
  let prev_href = format!("?page={}", page_num - 1);
  let next_href = format!("?page={}", page_num + 1);
  let store = crate::app::utils::object_store(cx);
  let products =
    crate::app::admin::produtos::resolve_summary_images(&store, products).await;

  let collection_name = collection.name.clone();
  let collection_description = collection.description.clone();

  // Prepare toolbar parameters
  let collection_url = href!(page, Slug(slug.to_string())).resolve(cx);
  let toolbar_sort = sort.clone();
  let toolbar_tamanho = tamanho.clone();
  let toolbar_cor = cor.clone();
  let toolbar_preco_min = preco_min.map(|s| s.to_string());
  let toolbar_preco_max = preco_max.map(|s| s.to_string());
  let toolbar_colors = available_colors.clone();

  Ok(view! {
        container(
            breadcrumb(
                breadcrumb_list(
                    breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::page)) }, "Início"))
                    breadcrumb_separator()
                    breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::colecoes::page)) }, "Coleções"))
                    breadcrumb_separator()
                    breadcrumb_item(breadcrumb_page((collection_name.clone())))
                )
            )

            <h1 class="text-4xl font-bold tracking-tight">(collection_name.clone())</h1>
            if let Some(description) = collection_description {
                <p class="max-w-2xl text-muted-foreground">(description)</p>
            }
            <p class="text-sm text-muted-foreground">(product_count) " peças nesta coleção"</p>

            // Filter toolbar
            catalog_toolbar(
                category: String::new(),
                search: String::new(),
                sort: toolbar_sort,
                tamanho: toolbar_tamanho,
                cor: toolbar_cor,
                preco_min: toolbar_preco_min,
                preco_max: toolbar_preco_max,
                disponivel: disponivel,
                sale: sale,
                available_colors: toolbar_colors,
                form_action: collection_url,
            )

            <div class="grid gap-4 @sm/page:grid-cols-2 @2xl/page:grid-cols-3">
                #[key(product.id.to_string())]
                for product in products {
                    let image_url = product
                        .image_url
                        .clone()
                        .unwrap_or_else(|| "/static/product-fallback.svg".to_string());
                    let name = product.name.clone();
                    let product_url = href!(
                      crate::app::produtos::slug::page,
                      crate::app::produtos::slug::Slug(product.slug.clone())
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
            if show_pagination {
                pagination(
                    pagination_content(
                        if has_prev {
                            pagination_item(
                                <a href=(prev_href) class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))>
                                    "Anterior"
                                </a>
                            )
                        }
                        pagination_item(
                            <span class="px-3 text-sm text-muted-foreground">(page_label)</span>
                        )
                        if has_next {
                            pagination_item(
                                <a href=(next_href) class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))>
                                    "Próxima"
                                </a>
                            )
                        }
                    )
                )
            }
        )
    }.boxed())
}
