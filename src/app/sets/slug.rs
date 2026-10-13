use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{
    content::Form, error::bad_request, href, page, path_param, query_params,
  },
  runtime::{Event, signal},
  view::{View, ViewExt, attributes, class, view},
};

use crate::app::store::cart::{
  CartError, add_set_to_cart, add_set_to_cart_proc, load_or_create_cart,
};

use crate::app::store::pricing::{
  discount_percent, pix_price, storefront_settings,
};
use crate::app::store::product_card::product_card;
use crate::app::store::queries::{
  count_set_members, format_price, get_set_by_slug, list_set_images,
  list_set_members, set_available_quantity,
};
use crate::app::utils::{object_store, resolve_storage_url};
use crate::auth::user::current_user_owned;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::breadcrumb::{
  breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list,
  breadcrumb_page, breadcrumb_separator,
};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::container::container;

path_param!(pub(crate) slug);

#[derive(Deserialize)]
pub struct AddSetInput {
  quantity: Option<String>,
}

#[query_params(error = bad_request)]
struct SetQuery {
  img: Option<String>,
}

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<AddSetInput>>,
) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let slug = path_param::<Slug>(cx);
  let store = object_store(cx);

  let set = match get_set_by_slug(pool, slug).await? {
        Some(s) => s,
        None => {
            return Ok(view! {
                container(
                    <h1 class="text-2xl font-bold">"Conjunto não encontrado"</h1>
                    <a
                        href=(href!(crate::app::sets::page))
                        class=(class!(button_variants(ButtonVariant::Primary, ButtonSize::Md), "mx-auto mt-6 w-fit"))
                    >
                        "Ver conjuntos"
                    </a>
                )
            }.boxed())
        }
    };

  let images = list_set_images(pool, set.id).await?;
  let mut gallery = Vec::with_capacity(images.len());
  for image in images {
    if let Some(url) = resolve_storage_url(&store, Some(&image.url)).await {
      gallery.push((image.id.to_string(), url, image.alt.clone()));
    }
  }

  let members = list_set_members(pool, set.id).await?;
  let members =
    crate::app::admin::produtos::resolve_summary_images(&store, members).await;
  let member_count = count_set_members(pool, &set.id).await?;
  let available = set_available_quantity(pool, set.id).await?;
  let settings = storefront_settings(cx).await;

  let query = query_params::<SetQuery>(cx)
    .map_err(|err| bad_request(format!("consulta inválida: {err}")))?;
  let initial_img_id = gallery
    .iter()
    .find(|(id, _, _)| query.img.as_deref() == Some(id.as_str()))
    .or_else(|| gallery.first())
    .map(|(id, _, _)| id.clone())
    .unwrap_or_default();

  let set_name = set.name.clone();
  let set_slug = set.slug.clone();
  let set_description = set.description.clone();
  let price = set.price_in_cents;
  let compare_at = set.compare_at_price_in_cents;
  let pix_discount = settings.pix_discount_percent;
  let pix_cents = price.map(|p| {
    if pix_discount > 0 {
      pix_price(p, pix_discount)
    } else {
      p
    }
  });
  let discount_badge = price
    .and_then(|p| discount_percent(compare_at, p))
    .map(|pct| format!("-{pct}%"));
  let sold_out = available <= 0;

  // No-JS fallback: the form posts back to this page, which adds the whole
  // fixed-composition bundle directly. With JS, the `@submit` handler below
  // calls the `add_set_to_cart_proc` procedure instead for an inline update.
  let mut added_to_cart = false;
  let mut add_error: Option<String> = None;
  if let Some(Form(input)) = body {
    let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
    let quantity = input
      .quantity
      .as_deref()
      .unwrap_or("1")
      .trim()
      .parse::<i32>()
      .unwrap_or(1);
    let cart = load_or_create_cart(cx, pool, user_id).await?;
    match add_set_to_cart(pool, cart.id, set.id, quantity).await {
      Ok(()) => added_to_cart = true,
      Err(CartError::InvalidVariant) => {
        add_error = Some("Este conjunto não está disponível.".to_string());
      }
      Err(CartError::OutOfStock) => {
        add_error = Some("Este conjunto está esgotado.".to_string());
      }
      Err(CartError::Db(error)) => return Err(error.into()),
    }
  }

  let added = signal(cx, || added_to_cart);
  let add_error_sig = signal(cx, || add_error.clone().unwrap_or_default());
  let selected_img = signal(cx, move || initial_img_id.clone());
  let add_set_id = set.id.to_string();
  let add_quantity = "1".to_string();
  let empty_error = String::new();

  Ok(view! {
      container(
          breadcrumb(
              breadcrumb_list(
                  breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::page)) }, "Início"))
                  breadcrumb_separator()
                  breadcrumb_item(breadcrumb_link(attrs: attributes! { href=(href!(crate::app::sets::page)) }, "Conjuntos"))
                  breadcrumb_separator()
                  breadcrumb_item(breadcrumb_page((set_name.clone())))
              )
          )

          <h1 class="text-4xl font-bold tracking-tight">(set_name.clone())</h1>
          if let Some(description) = set_description {
              <p class="max-w-2xl text-muted-foreground">(description)</p>
          }
          <p class="text-sm text-muted-foreground">(member_count) " peças neste conjunto"</p>

          <div class="grid gap-4 @3xl/page:grid-cols-2">
              <div class="space-y-4">
                  <div class="relative overflow-hidden rounded-xl border border-border">
                      <div class="relative aspect-[4/5] bg-foreground/5">
                          if gallery.is_empty() {
                              <img src="/static/product-fallback.svg" alt=(set_name.clone()) class="absolute inset-0 h-full w-full object-cover">
                          } else {
                              for (id, url, alt) in gallery.clone() {
                                  let layer_id = id.clone();
                                  let layer_hidden = id.clone();
                                  <img
                                      src=(url)
                                      alt=(alt.unwrap_or_else(|| set_name.clone()))
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
                  </div>
                  if gallery.len() > 1 {
                      <div class="grid grid-cols-4 gap-3" role="listbox" aria-label="Imagens do conjunto">
                          for (id, url, alt) in gallery {
                              let thumb_id = id.clone();
                              let select_id = id.clone();
                              let pressed_id = id.clone();
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
                                  <img src=(url) alt=(alt.unwrap_or_else(|| set_name.clone())) class="h-full w-full object-cover">
                              </button>
                          }
                      </div>
                  }
              </div>

              <div class="space-y-4">
                  if let Some(price_cents) = price {
                      <div class="space-y-2">
                          <div class="flex items-center gap-2">
                              if let Some(discount_text) = discount_badge.clone() {
                                  badge(
                                      variant: BadgeVariant::Primary,
                                      (discount_text)
                                  )
                              }
                              if sold_out {
                                  badge(variant: BadgeVariant::Destructive, "Esgotado")
                              } else {
                                  badge(variant: BadgeVariant::Primary, "Em estoque")
                              }
                          </div>
                          if let Some(compare) = compare_at {
                              <p class="text-sm text-muted-foreground line-through">
                                  (format_price(compare))
                              </p>
                          }
                          if pix_discount > 0 {
                              <p class="text-2xl font-bold text-primary">
                                  (format!("{} no Pix", format_price(pix_cents.unwrap_or(price_cents))))
                              </p>
                              <p class="text-sm text-muted-foreground">
                                  "ou " (format_price(price_cents))
                              </p>
                          } else {
                              <p class="text-2xl font-medium text-primary">(format_price(price_cents))</p>
                          }
                          if sold_out {
                              <p class="text-sm text-muted-foreground">"Este conjunto está esgotado no momento."</p>
                          } else {
                              <p class="text-sm text-muted-foreground">(available) " disponíveis"</p>
                          }
                      </div>

                      <form
                          method="post"
                          action=(href!(page, Slug(set_slug.clone())))
                          class="space-y-3"
                          @submit=$(async |e: Event| {
                              e.prevent_default();
                              let n = add_set_to_cart_proc(add_set_id.to_owned(), add_quantity.to_owned()).await;
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
                          <input type="hidden" name="quantity" value="1">
                          <button
                              type="submit"
                              disabled=(sold_out)
                              class=(class!(button_variants(ButtonVariant::Primary, ButtonSize::Lg), "w-full"))
                          >
                              if sold_out {
                                  "Indisponível"
                              } else {
                                  "Adicionar conjunto"
                              }
                          </button>
                          <p class="text-sm text-primary" :hidden=$(!added.get())>
                              "Adicionado ao carrinho!"
                          </p>
                          <p class="text-sm text-destructive" :hidden=$(add_error_sig.get() == "")>
                              $(add_error_sig.get())
                          </p>
                      </form>
                  } else if sold_out {
                      badge(variant: BadgeVariant::Destructive, "Esgotado")
                  }
              </div>
          </div>

          if !members.is_empty() {
              <section>
                  <p class="text-sm font-medium tracking-tight text-primary uppercase">"O conjunto"</p>
                  <h2 class="mt-2 mb-4 text-4xl font-bold tracking-tight">"Peças inclusas"</h2>
                  <div class="grid gap-8 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                      #[key(item.id.to_string())]
                      for item in members {
                          product_card(product: item)
                      }
                  </div>
              </section>
          }
      )
  }.boxed())
}
