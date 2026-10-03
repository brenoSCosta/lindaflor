use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{content::Form, page, path_param, query_params},
  view::{View, ViewExt, attributes, class, view},
};

use crate::app::store::cart::{CartItem, add_to_cart};
use crate::app::store::queries::{
  DEFAULT_WHATSAPP_NUMBER, category_label, format_price, get_product_by_slug,
  get_store_settings, list_products, render_whatsapp_template, size_label,
  urlencode,
};
use crate::components::accordion::{
  accordion, accordion_content, accordion_item, accordion_trigger,
};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::separator::separator;

path_param!(slug);

#[derive(Deserialize)]
pub struct AddToCartInput {
  variant_id: Option<String>,
}

#[query_params(error = bad_request)]
struct ProductQuery {
  variant: Option<String>,
  img: Option<String>,
}

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<AddToCartInput>>,
) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let slug = path_param::<Slug>(cx);

  let mut added_to_cart = false;
  if let Some(Form(input)) = body
    && let Some(variant_id) =
      input.variant_id.as_deref().filter(|v| !v.is_empty())
    && let Some(product) = get_product_by_slug(pool, slug).await?
    && let Some(variant) = product
      .variants
      .iter()
      .find(|v| v.id.to_string() == variant_id)
    && variant.available > 0
  {
    let image_url = product.images.first().map(|i| i.url.clone());
    add_to_cart(
      cx,
      CartItem {
        variant_id: variant.id.to_string(),
        product_id: product.id.to_string(),
        product_slug: product.slug.clone(),
        product_name: product.name.clone(),
        variant_label: format!(
          "{} · {}",
          size_label(&variant.size),
          variant.color
        ),
        image_url,
        unit_price_cents: variant
          .price_in_cents
          .unwrap_or(product.price_in_cents),
        quantity: 1,
        max_quantity: variant.available,
      },
    );
    added_to_cart = true;
  }

  let product = match get_product_by_slug(pool, slug).await? {
        Some(p) => p,
        None => {
            return Ok(view! {
                <div class="mx-auto max-w-7xl px-4 py-24 text-center md:px-8">
                    <p class="text-2xl font-bold">"Produto não encontrado"</p>
                    <a
                        href="/produtos"
                        class=(class!(button_variants(ButtonVariant::Primary, ButtonSize::Md), "mx-auto mt-6 w-fit"))
                    >
                        "Voltar ao catálogo"
                    </a>
                </div>
            }.boxed())
        }
    };

  let settings = get_store_settings(pool).await?;
  let related =
    list_products(pool, Some(&product.category), None, false, true, None)
      .await?;
  let related: Vec<_> = related
    .into_iter()
    .filter(|p| p.slug != product.slug)
    .take(4)
    .collect();

  let product_query = query_params::<ProductQuery>(cx)?;
  let selected_variant = product
    .variants
    .iter()
    .find(|v| {
      product_query.variant.as_deref() == Some(v.id.to_string().as_str())
    })
    .or_else(|| product.variants.iter().find(|v| v.available > 0))
    .or_else(|| product.variants.first())
    .cloned();

  let price = selected_variant
    .as_ref()
    .and_then(|v| v.price_in_cents)
    .unwrap_or(product.price_in_cents);

  let main_image = product
    .images
    .iter()
    .find(|i| product_query.img.as_deref() == Some(i.id.to_string().as_str()))
    .or_else(|| product.images.first())
    .cloned();

  let main_image_url = main_image.as_ref().map(|i| i.url.clone());
  let main_image_alt = main_image
    .as_ref()
    .and_then(|i| i.alt.clone())
    .unwrap_or_else(|| product.name.clone());
  let main_image_id = main_image.as_ref().map(|i| i.id.to_string());

  let whatsapp_number = settings
    .whatsapp_number
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .unwrap_or(DEFAULT_WHATSAPP_NUMBER)
    .to_string();
  let variant_label = selected_variant
    .as_ref()
    .map(|v| format!("{} · {}", size_label(&v.size), v.color))
    .unwrap_or_default();
  let whatsapp_fallback = format!(
    "Olá! Tenho interesse no {}{}",
    product.name,
    if variant_label.is_empty() {
      String::new()
    } else {
      format!(" ({})", variant_label)
    }
  );
  let whatsapp_message = render_whatsapp_template(
    settings.whatsapp_message_template.as_deref(),
    &[
      ("product", product.name.as_str()),
      ("variant", variant_label.as_str()),
    ],
    &whatsapp_fallback,
  );
  let whatsapp_url = format!(
    "https://wa.me/{}?text={}",
    whatsapp_number,
    urlencode(&whatsapp_message)
  );

  let product_name = product.name.clone();
  let product_slug = product.slug.clone();
  let product_category = category_label(&product.category).to_string();
  let product_description = product.description.clone();
  let product_images = product.images.clone();
  let product_variants = product.variants.clone();
  let selected_variant_id = selected_variant.as_ref().map(|v| v.id.to_string());
  let selected_available =
    selected_variant.as_ref().map(|v| v.available).unwrap_or(0);
  let selected_variant_id_attr =
    selected_variant_id.clone().unwrap_or_default();

  Ok(view! {
        <main class="mx-auto max-w-7xl px-4 py-10 md:px-8 md:py-16">
            breadcrumb(
                breadcrumb_list(
                    breadcrumb_item(breadcrumb_link(attrs: attributes! { href="/" }, "Início"))
                    breadcrumb_separator()
                    breadcrumb_item(breadcrumb_link(attrs: attributes! { href="/produtos" }, "Catálogo"))
                    breadcrumb_separator()
                    breadcrumb_item(breadcrumb_page((product_name.clone())))
                )
            )

            <div class="mt-8 grid gap-12 lg:grid-cols-2 lg:gap-16">
                <div class="space-y-4">
                    <div class="overflow-hidden rounded-xl border border-border">
                        <div class="aspect-[4/5]">
                            if let Some(url) = main_image_url {
                                <img src=(url) alt=(main_image_alt) class="h-full w-full object-cover">
                            }
                        </div>
                    </div>
                    if product_images.len() > 1 {
                        <div class="grid grid-cols-4 gap-3">
                            for image in product_images {
                                <a
                                    href=(format!("/produtos/{}?img={}", product_slug, image.id))
                                    class=(class!(
                                        "aspect-square overflow-hidden rounded-lg border",
                                        "border-primary" if main_image_id.as_deref() == Some(image.id.to_string().as_str()) else "border-border",
                                    ))
                                >
                                    <img src=(image.url) alt=(image.alt.unwrap_or_else(|| product_name.clone())) class="h-full w-full object-cover">
                                </a>
                            }
                        </div>
                    }
                </div>

                <div class="space-y-8 lg:pt-8">
                    card(
                        card_header(
                            badge(variant: BadgeVariant::Secondary, (product_category))
                            card_title((product_name.clone()))
                        )
                        card_content(
                            <p class="text-2xl font-medium text-primary">(format_price(price))</p>
                            if let Some(description) = product_description {
                                <p class="text-muted-foreground">(description)</p>
                            }
                        )
                    )

                    card(
                        card_header(
                            card_title("Selecione tamanho e cor")
                        )
                        card_content(
                            <div class="grid gap-2">
                                for variant in product_variants {
                                    let is_selected = selected_variant_id.as_deref() == Some(variant.id.to_string().as_str());
                                    let disabled = variant.available == 0;
                                    <a
                                        href=(format!("/produtos/{}?variant={}", product_slug, variant.id))
                                        class=(class!(
                                            button_variants(
                                                if is_selected { ButtonVariant::Primary } else { ButtonVariant::Outline },
                                                ButtonSize::Md,
                                            ),
                                            "w-full justify-between",
                                            "opacity-50" if disabled,
                                        ))
                                    >
                                        <span>
                                            <span class="font-medium">(size_label(&variant.size)) " · " (variant.color)</span>
                                            <span class="block text-xs">"SKU " (variant.sku)</span>
                                        </span>
                                        <span class="text-sm">
                                            if variant.available > 0 {
                                                (variant.available) " disponíveis"
                                            } else {
                                                "Esgotado"
                                            }
                                        </span>
                                    </a>
                                }
                            </div>
                        )
                    )

                    card(
                        card_content(
                            <form method="post" action=(format!("/produtos/{}", product_slug)) class="space-y-3">
                                <input type="hidden" name="variant_id" value=(selected_variant_id_attr)>
                                button(
                                    variant: ButtonVariant::Primary,
                                    size: ButtonSize::Lg,
                                    attrs: attributes! { type="submit" disabled=(selected_available == 0) class="w-full" },
                                    if selected_available > 0 {
                                        "Adicionar ao carrinho"
                                    } else {
                                        "Indisponível"
                                    }
                                )
                                <a
                                    href=(whatsapp_url)
                                    target="_blank"
                                    rel="noreferrer"
                                    class=(class!(button_variants(ButtonVariant::Outline, ButtonSize::Lg), "w-full"))
                                >
                                    "Comprar pelo WhatsApp"
                                </a>
                                if added_to_cart {
                                    <p class="text-sm text-primary">"Adicionado ao carrinho!"</p>
                                }
                                if selected_available > 0 {
                                    badge(variant: BadgeVariant::Primary, "Em estoque")
                                } else {
                                    badge(variant: BadgeVariant::Destructive, "Esgotado")
                                }
                            </form>
                        )
                    )

                    separator(attrs: attributes! { class="my-4" })

                    accordion(
                        accordion_item(
                            attrs: attributes! { open=(true) },
                            accordion_trigger("Frete e entrega")
                            accordion_content(
                                <p>"Frete grátis para compras acima de R$ 299. Para Sergipe, prazo de 3 a 7 dias úteis. Demais estados, 7 a 15 dias úteis."</p>
                            )
                        )
                        accordion_item(
                            accordion_trigger("Trocas e devoluções")
                            accordion_content(
                                <p>
                                    "Você tem até 7 dias após o recebimento para solicitar troca ou devolução. Consulte nossa "
                                    <a href="/trocas-devolucoes" class="text-primary underline">"política completa"</a>
                                    "."
                                </p>
                            )
                        )
                        accordion_item(
                            accordion_trigger("Cuidados com a peça")
                            accordion_content(
                                <p>"Lave à mão com água fria. Não use alvejante. Seque à sombra para preservar cores e elasticidade."</p>
                            )
                        )
                    )
                </div>
            </div>

            if !related.is_empty() {
                <section class="mt-24 border-t border-border pt-16">
                    <p class="text-sm font-medium tracking-tight text-primary uppercase">"Você também pode gostar"</p>
                    <h2 class="mt-2 mb-10 text-4xl font-bold tracking-tight">"Complete o look"</h2>
                    <div class="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
                        #[key(item.id.to_string())]
                        for item in related {
                            let image_url = item
                                .image_url
                                .clone()
                                .unwrap_or_else(|| "/static/product-fallback.svg".to_string());
                            let name = item.name.clone();
                            let item_url = format!("/produtos/{}", item.slug);
                            let category_text = category_label(&item.category).to_string();
                            let price_text = format_price(item.price_in_cents);

                            card(
                                attrs: attributes! { class="group overflow-hidden" },
                                <a href=(item_url) class="block">
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
                                )
                            )
                        }
                    </div>
                </section>
            }
        </main>
    }.boxed())
}
