use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{content::Form, href, page, path_param, query_params},
  runtime::{Event, signal},
  view::{View, ViewExt, attributes, class, view},
};
use uuid::Uuid;

use crate::app::store::cart::{
  CartError, add_item, add_to_cart_proc, load_or_create_cart,
};
use crate::app::store::pricing::{discount_percent, pix_price};
use crate::app::store::product_card::product_card;
use crate::app::store::queries::{
  category_label, format_price, get_product_by_slug, get_store_settings,
  size_label, whatsapp_link,
};
use crate::app::store::wishlist::{liked_product_ids, wishlist_button};
use crate::auth::user::current_user_owned;
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
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::container;
use crate::components::separator::separator;
use topcoat::icon::{icon, iconify::iconify_icon};

path_param!(pub(crate) slug);

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
  let store = crate::app::utils::object_store(cx);
  let resolve_detail = crate::app::admin::produtos::resolve_detail_images;
  let resolve_summaries = crate::app::admin::produtos::resolve_summary_images;

  let mut added_to_cart = false;
  let mut add_error: Option<String> = None;
  if let Some(Form(input)) = body
    && let Some(variant_id) =
      input.variant_id.as_deref().filter(|v| !v.is_empty())
  {
    let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
    match Uuid::parse_str(variant_id) {
      Ok(variant_id) => {
        let cart = load_or_create_cart(cx, pool, user_id).await?;
        match add_item(pool, cart.id, variant_id, 1).await {
          Ok(()) => added_to_cart = true,
          Err(CartError::InvalidVariant) => {
            add_error = Some("Esta variação não está disponível.".to_string());
          }
          Err(CartError::OutOfStock) => {
            add_error = Some("Esta peça está esgotada.".to_string());
          }
          Err(CartError::Db(error)) => return Err(error.into()),
        }
      }
      Err(_) => {
        add_error = Some("Variação inválida.".to_string());
      }
    }
  }

  // Not-found renders below with a 200: keep it out of the index unless the
  // product loads (set_product_head then replaces this head).
  crate::seo::set_head(
    cx,
    crate::seo::PageHead::new("Produto não encontrado").noindex(),
  );
  let product = match get_product_by_slug(pool, slug).await? {
        Some(mut p) => {
            resolve_detail(&store, &mut p).await;
            p
        }
        None => {
            return Ok(view! {
                container(
                    attrs: attributes! { class="py-24 text-center" },
                    <p class="text-2xl font-bold">"Produto não encontrado"</p>
                    <a
                        href=(href!(crate::app::produtos::page))
                        class=(class!(button_variants(ButtonVariant::Primary, ButtonSize::Md), "mx-auto mt-6 w-fit"))
                    >
                        "Voltar ao catálogo"
                    </a>
                )
            }.boxed())
        }
    };
  crate::seo::structured::set_product_head(cx, &product);

  let settings = get_store_settings(pool).await?;
  let shipping_copy = if settings.free_shipping_threshold_cents <= 0 {
    "Frete grátis em todas as compras. Para Sergipe, prazo de 3 a 7 dias úteis. Demais estados, 7 a 15 dias úteis.".to_string()
  } else {
    format!(
      "Frete grátis para compras acima de {}. Para Sergipe, prazo de 3 a 7 dias úteis. Demais estados, 7 a 15 dias úteis.",
      format_price(settings.free_shipping_threshold_cents)
    )
  };
  let related = crate::app::store::queries::list_related_products(
    pool,
    product.id,
    &product.category,
  )
  .await?;
  let related = resolve_summaries(&store, related).await;

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

  let initial_img_id = product
    .images
    .iter()
    .find(|i| product_query.img.as_deref() == Some(i.id.to_string().as_str()))
    .or_else(|| product.images.first())
    .map(|i| i.id.to_string())
    .unwrap_or_default();

  let variant_label = selected_variant
    .as_ref()
    .map(|v| format!("{} · {}", size_label(&v.size), v.color))
    .unwrap_or_default();
  let price_text = format_price(price);
  let whatsapp_message = if variant_label.is_empty() {
    format!(
      "Olá! Tenho interesse no {} no valor de {}.",
      product.name, price_text
    )
  } else {
    format!(
      "Olá! Tenho interesse no {} ({}) no valor de {}.",
      product.name, variant_label, price_text
    )
  };
  let whatsapp_url = settings
    .whatsapp_number
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(|number| whatsapp_link(number, &whatsapp_message));

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
  let initial_color = selected_variant
    .as_ref()
    .map(|v| v.color.clone())
    .unwrap_or_default();
  let selected_color = signal(cx, || initial_color.clone());
  let added = signal(cx, || added_to_cart);
  let add_error_sig = signal(cx, || add_error.clone().unwrap_or_default());
  let selected_img = signal(cx, move || initial_img_id.clone());
  let add_variant_id = selected_variant_id_attr.clone();
  let empty_error = String::new();

  let pix_discount_percent = settings.pix_discount_percent;
  let pix_price_cents = if pix_discount_percent > 0 {
    pix_price(price, pix_discount_percent)
  } else {
    0
  };
  let discount_badge_text =
    discount_percent(product.compare_at_price_in_cents, product.price_in_cents)
      .map(|pct| format!("-{pct}%"));

  let looks =
    crate::app::store::queries::list_product_looks(pool, product.id).await?;
  let looks = resolve_summaries(&store, looks).await;

  let liked = liked_product_ids(cx).await.contains(&product.id);
  let product_id = product.id.to_string();

  Ok(view! {
        container(
            breadcrumb(
                breadcrumb_list(
                    breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::page)) }, "Início"))
                    breadcrumb_separator()
                    breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::produtos::page)) }, "Catálogo"))
                    breadcrumb_separator()
                    breadcrumb_item(breadcrumb_page((product_name.clone())))
                )
            )

            <div class="grid gap-4 @3xl/page:grid-cols-2">
                <div class="space-y-4">
                    <div class="relative overflow-hidden rounded-xl border border-border">
                        <div class="relative aspect-[4/5] bg-foreground/5">
                            if product_images.is_empty() {
                                <div class="flex h-full w-full items-center justify-center">
                                    icon(data: iconify_icon!("lucide:image"))
                                </div>
                            } else {
                                for image in product_images.clone() {
                                    let layer_id = image.id.to_string();
                                    let layer_hidden = image.id.to_string();
                                    <img
                                        src=(image.url)
                                        alt=(image.alt.unwrap_or_else(|| product_name.clone()))
                                        :class=$(if selected_img.get() == layer_id {
                                            "absolute inset-0 z-10 h-full w-full object-cover opacity-100 transition-opacity duration-700 ease-in-out"
                                        } else {
                                            "pointer-events-none absolute inset-0 z-0 h-full w-full object-cover opacity-0 transition-opacity duration-700 ease-in-out"
                                        })
                                        :aria-hidden=$(if selected_img.get() == layer_hidden { "false" } else { "true" })
                                    >
                                }
                            }
                        </div>
                        <div class="absolute right-4 top-4 z-20">
                            wishlist_button(product_id: product_id.clone(), liked: liked, next: String::new())
                        </div>
                    </div>
                    if product_images.len() > 1 {
                        <div class="grid grid-cols-4 gap-3" role="listbox" aria-label="Imagens do produto">
                            for image in product_images {
                                let thumb_id = image.id.to_string();
                                let select_id = image.id.to_string();
                                let pressed_id = image.id.to_string();
                                <button
                                    type="button"
                                    role="option"
                                    :aria-selected=$(if selected_img.get() == pressed_id { "true" } else { "false" })
                                    :class=$(if selected_img.get() == thumb_id {
                                        "aspect-square overflow-hidden rounded-lg border border-primary ring-1 ring-primary/30 transition-[border-color,box-shadow,opacity] duration-300"
                                    } else {
                                        "aspect-square overflow-hidden rounded-lg border border-border opacity-80 transition-[border-color,box-shadow,opacity] duration-300 hover:border-primary/40 hover:opacity-100"
                                    })
                                    @click=$(|_e: Event| {
                                        selected_img.set(select_id.to_owned());
                                        let img_id = selected_img.get();
                                        raw!(
                                            r#"const url = new URL(window.location.href);
                                               url.searchParams.set("img", String(${img_id}));
                                               history.replaceState(null, "", url.pathname + url.search);"#,
                                            {
                                                let _ = img_id;
                                            }
                                        );
                                    })
                                >
                                    <img src=(image.url) alt=(image.alt.unwrap_or_else(|| product_name.clone())) class="h-full w-full object-cover">
                                </button>
                            }
                        </div>
                    }
                </div>

                <div class="space-y-4">
                    card(
                        card_header(
                            <div class="flex items-start justify-between">
                                <div class="space-y-2">
                                    badge(variant: BadgeVariant::Secondary, (product_category))
                                    card_title((product_name.clone()))
                                </div>
                                if let Some(discount_text) = discount_badge_text.clone() {
                                    badge(
                                        variant: BadgeVariant::Primary,
                                        (discount_text)
                                    )
                                }
                            </div>
                        )
                        card_content(
                            <div class="space-y-2">
                                if let Some(compare_at) = product.compare_at_price_in_cents {
                                    <p class="text-sm text-muted-foreground line-through">
                                        (format_price(compare_at))
                                    </p>
                                }
                                if pix_discount_percent > 0 {
                                    <p class="text-2xl font-bold text-primary">
                                        (format!("{} no Pix", format_price(pix_price_cents)))
                                    </p>
                                    <p class="text-sm text-muted-foreground">
                                        "ou " (format_price(price))
                                    </p>
                                } else {
                                    <p class="text-2xl font-medium text-primary">(format_price(price))</p>
                                }
                            </div>
                            if let Some(description) = product_description {
                                <p class="text-muted-foreground mt-4">(description)</p>
                            }
                        )
                    )

                    if !product.colors.is_empty() {
                        card(
                            card_header(
                                card_title("Selecione a cor")
                            )
                            card_content(
                                <div class="flex gap-3 flex-wrap items-center" role="radiogroup" aria-label="Cores disponíveis">
                                    for (color_name, color_hex) in product.colors.iter().take(12) {
                                        let set_color = color_name.clone();
                                        let pressed_color = color_name.clone();
                                        let ring_color = color_name.clone();
                                        let swatch_style = color_hex.clone().map(|hex| format!("background-color: {hex}"));
                                        <button
                                            type="button"
                                            role="radio"
                                            title=(color_name)
                                            aria-label=(color_name)
                                            :aria-checked=$(if selected_color.get() == pressed_color { "true" } else { "false" })
                                            :class=$(if selected_color.get() == ring_color {
                                                "h-8 w-8 rounded-full border-2 border-primary ring-2 ring-primary/30 transition-[border-color,box-shadow] duration-300"
                                            } else {
                                                "h-8 w-8 rounded-full border-2 border-border transition-[border-color,box-shadow] duration-300 hover:border-primary/40"
                                            })
                                            @click=$(|_e: Event| {
                                                selected_color.set(set_color.to_owned());
                                            })
                                        >
                                            if let Some(style) = swatch_style {
                                                <span class="block h-full w-full rounded-full" style=(style)></span>
                                            } else {
                                                <span class="block h-full w-full rounded-full bg-foreground/10"></span>
                                            }
                                        </button>
                                    }
                                    if product.colors.len() > 12 {
                                        <span class="text-sm text-muted-foreground">
                                            (format!("+{} cores", product.colors.len() - 12))
                                        </span>
                                    }
                                    <button
                                        type="button"
                                        class="text-xs text-primary hover:underline"
                                        @click=$(|_e: Event| {
                                            selected_color.set("".to_owned());
                                        })
                                    >
                                        "Todas"
                                    </button>
                                </div>
                            )
                        )
                    }

                    card(
                        card_header(
                            <div class="flex items-center justify-between">
                                card_title("Selecione o tamanho")
                                <a
                                    href=(href!(crate::app::guia_de_medidas::page))
                                    class="text-xs text-primary hover:underline"
                                >
                                    "Guia de medidas"
                                </a>
                            </div>
                        )
                        card_content(
                            <p class="mb-2 text-sm text-muted-foreground">
                                "Cor selecionada: "
                                <span class="font-medium">$(if selected_color.get().is_empty() { "Todas".to_owned() } else { selected_color.get() })</span>
                            </p>
                            <div class="grid gap-2">
                                for variant in product_variants {
                                    let is_selected = selected_variant_id.as_deref() == Some(variant.id.to_string().as_str());
                                    let disabled = variant.available == 0;
                                    let variant_id = variant.id.to_string();
                                    let hidden_color = variant.color.clone();
                                    <a
                                        href=(href!(page, Slug(product_slug.clone())).query([("variant", variant_id.as_str())]))
                                        :hidden=$(if selected_color.get().is_empty() { false } else { selected_color.get() != hidden_color })
                                        class=(class!(
                                            button_variants(
                                                if is_selected { ButtonVariant::Primary } else { ButtonVariant::Outline },
                                                ButtonSize::Md,
                                            ),
                                            "w-full justify-between",
                                            "opacity-50" if disabled,
                                        ))
                                    >
                                        <span class="font-medium">(size_label(&variant.size)) " · " (variant.color)</span>
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
                            <form
                                method="post"
                                action=(href!(page, Slug(product_slug.clone())))
                                class="space-y-3"
                                @submit=$(async |e: Event| {
                                    e.prevent_default();
                                    let n = add_to_cart_proc(add_variant_id.to_owned()).await;
                                    added.set(true);
                                    add_error_sig.set(empty_error.clone());
                                    raw!(
                                        "(() => { const badge = document.querySelector('[data-cart-count]'); if (!badge) return; badge.textContent = ${n}; badge.hidden = ${n} === '0'; })()",
                                        {
                                            let _ = n.clone();
                                        }
                                    );
                                    raw!(
                                        "(() => { const opener = document.querySelector('[data-cart-open]'); if (opener) opener.click(); })();"
                                    );
                                })
                            >
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
                                if let Some(url) = whatsapp_url {
                                  <a
                                      href=(url)
                                      target="_blank"
                                      rel="noreferrer"
                                      class=(class!(button_variants(ButtonVariant::Outline, ButtonSize::Lg), "w-full"))
                                  >
                                      "Comprar pelo WhatsApp"
                                  </a>
                                }
                                <p class="text-sm text-primary" :hidden=$(!added.get())>
                                    "Adicionado ao carrinho!"
                                </p>
                                <p class="text-sm text-destructive" :hidden=$(add_error_sig.get() == "")>
                                    $(add_error_sig.get())
                                </p>
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
                                <p>(shipping_copy)</p>
                            )
                        )
                        accordion_item(
                            accordion_trigger("Trocas e devoluções")
                            accordion_content(
                                <p>
                                    "Você tem até 7 dias após o recebimento para solicitar troca ou devolução. Consulte nossa "
                                    <a href=(href!(crate::app::trocas_devolucoes::page)) class="text-primary underline">"política completa"</a>
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

            if !looks.is_empty() {
                <section>
                    separator(attrs: attributes! { class="my-4" })
                    <p class="text-sm font-medium tracking-tight text-primary uppercase">"Complementos"</p>
                    <h2 class="mt-2 mb-4 text-4xl font-bold tracking-tight">"Complete o look"</h2>
                    <div class="grid gap-8 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                        #[key(item.id.to_string())]
                        for item in looks {
                            product_card(product: item)
                        }
                    </div>
                </section>
            }

            if !related.is_empty() {
                <section>
                    separator(attrs: attributes! { class="my-4" })
                    <p class="text-sm font-medium tracking-tight text-primary uppercase">"Você também pode gostar"</p>
                    <h2 class="mt-2 mb-4 text-4xl font-bold tracking-tight">"Produtos relacionados"</h2>
                    <div class="grid gap-8 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                        #[key(item.id.to_string())]
                        for item in related {
                            product_card(product: item)
                        }
                    </div>
                </section>
            }
        )
    }.boxed())
}
