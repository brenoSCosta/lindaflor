use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page, path_param},
  view::{View, ViewExt, attributes, class, view},
};

use crate::app::store::queries::{
  category_label, count_collection_products, format_price,
  get_collection_by_slug, list_products,
};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{card, card_footer, card_header, card_title};
use crate::components::container::container;

path_param!(pub(crate) slug);

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
  let products =
    list_products(pool, None, None, false, true, Some(slug)).await?;
  let store = crate::app::utils::object_store(cx);
  let products =
    crate::app::admin::produtos::resolve_summary_images(&store, products).await;

  let collection_name = collection.name.clone();
  let collection_description = collection.description.clone();

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
        )
    }.boxed())
}
