use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{content::Form, href, page},
  runtime::Event,
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::auth::user::current_user_owned;
use crate::components::alert::{AlertVariant, alert, alert_title};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::separator::separator;

use crate::app::store::cart::{
  cart_coupon_code, cart_subtotal_cents, hydrate_cart, load_or_create_cart,
  remove_from_cart_proc, remove_item, set_cart_coupon, set_cart_quantity_proc,
  set_quantity,
};
use crate::app::store::coupons::{CouponReject, resolve_coupon};
use crate::app::store::queries::format_price;
use crate::components::field::{field, field_error, field_label};
use crate::components::input::input;

#[derive(Deserialize)]
pub struct CartUpdateInput {
  variant_id: Option<String>,
  quantity: Option<i32>,
  remove: Option<String>,
  #[serde(default)]
  coupon_code: Option<String>,
  #[serde(default)]
  intent: Option<String>,
}

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<CartUpdateInput>>,
) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let cart = load_or_create_cart(cx, pool, user_id).await?;

  let mut coupon_error: Option<String> = None;
  if let Some(Form(posted)) = body {
    match posted.intent.as_deref() {
      Some("apply_coupon") => {
        let raw = posted.coupon_code.clone().unwrap_or_default();
        if raw.trim().is_empty() {
          set_cart_coupon(pool, cart.id, None).await?;
        } else {
          let mut tx = pool.begin().await?;
          let subtotal = {
            let hydrated = hydrate_cart(pool, cart.id).await?;
            cart_subtotal_cents(&hydrated.items)
          };
          let outcome =
            resolve_coupon(&mut tx, &raw, subtotal, user_id, false).await;
          tx.rollback().await?;
          match outcome {
            Ok(Some(coupon)) => {
              set_cart_coupon(pool, cart.id, Some(&coupon.code)).await?;
            }
            Ok(None) => {
              set_cart_coupon(pool, cart.id, None).await?;
            }
            Err(CouponReject::Invalid(message)) => {
              set_cart_coupon(pool, cart.id, None).await?;
              coupon_error = Some(message);
            }
            Err(CouponReject::Db(error)) => return Err(error.into()),
          }
        }
      }
      Some("remove_coupon") => {
        set_cart_coupon(pool, cart.id, None).await?;
      }
      _ => {
        if let Some(variant_id) =
          posted.variant_id.as_deref().filter(|v| !v.is_empty())
          && let Ok(variant_id) = Uuid::parse_str(variant_id)
        {
          if posted.remove.as_deref() == Some("true") {
            remove_item(pool, cart.id, variant_id).await?;
          } else if let Some(quantity) = posted.quantity {
            let _ = set_quantity(pool, cart.id, variant_id, quantity).await;
          }
        }
      }
    }
  }

  let mut hydrated = hydrate_cart(pool, cart.id).await?;
  let store = crate::app::utils::object_store(cx);
  for item in &mut hydrated.items {
    item.image_url =
      crate::app::utils::resolve_storage_url(&store, item.image_url.as_deref())
        .await;
  }
  let items = hydrated.items;
  let notices = hydrated.notices;
  let subtotal = cart_subtotal_cents(&items);
  let stored_coupon = cart_coupon_code(pool, cart.id).await?;
  let coupon_input = stored_coupon.clone().unwrap_or_default();
  let mut preview_tx = pool.begin().await?;
  let coupon_outcome =
    resolve_coupon(&mut preview_tx, &coupon_input, subtotal, user_id, false)
      .await;
  preview_tx.rollback().await?;
  let (resolved_coupon, preview_error) = match coupon_outcome {
    Ok(coupon) => (coupon, None),
    Err(CouponReject::Db(error)) => return Err(error.into()),
    Err(CouponReject::Invalid(message)) => (None, Some(message)),
  };
  if coupon_error.is_none() {
    coupon_error = preview_error;
  }
  let discount = resolved_coupon
    .as_ref()
    .map(|coupon| coupon.discount_cents)
    .unwrap_or(0);
  let item_count: i32 = items.iter().map(|item| item.quantity).sum();
  let free_shipping_threshold = 29900;
  let missing_for_free_shipping = (free_shipping_threshold - subtotal).max(0);

  Ok(view! {
      container(
          variant: ContainerVariant::Narrow,
          <header class="flex items-end justify-between gap-4">
              <div>
                  <h1 class="text-4xl font-bold tracking-tight @md/page:text-5xl">"Carrinho"</h1>
                  if item_count > 0 {
                      <p class="mt-2 text-sm text-muted-foreground">
                          (item_count) " " if item_count == 1 { "peça" } else { "peças" }
                      </p>
                  }
              </div>
              if item_count > 0 {
                  badge(variant: BadgeVariant::Secondary, (item_count))
              }
          </header>

          if !notices.is_empty() {
              <div class="flex flex-col gap-2">
                  for notice in notices {
                      alert(
                          variant: AlertVariant::Neutral,
                          alert_title((notice.message()))
                      )
                  }
              </div>
          }

          if items.is_empty() {
              <div class="py-16 text-center">
                  <p class="text-muted-foreground">"Seu carrinho está vazio."</p>
                  <a
                      href=(href!(crate::app::produtos::page))
                      class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                  >
                      "Ver catálogo"
                  </a>
              </div>
          } else {
              <div class="flex flex-col gap-4">
                  for item in items {
                      card(
                          card_content(
                              attrs: attributes! { class="flex items-center gap-4" },
                              <a
                                  href=(href!(crate::app::produtos::slug::page, crate::app::produtos::slug::Slug(item.product_slug.clone())))
                                  class="shrink-0 overflow-hidden rounded-lg"
                              >
                                  if let Some(image_url) = &item.image_url {
                                      <img
                                          src=(image_url.clone())
                                          alt=(item.product_name.clone())
                                          class="h-24 w-20 object-cover"
                                      >
                                  }
                              </a>
                              <div class="min-w-0 flex-1 space-y-1">
                                  <a
                                      href=(href!(crate::app::produtos::slug::page, crate::app::produtos::slug::Slug(item.product_slug.clone())))
                                      class="text-sm font-medium uppercase tracking-wide transition-colors hover:text-primary"
                                  >
                                      (item.product_name)
                                  </a>
                                  <p class="text-sm text-muted-foreground">(item.variant_label)</p>
                                  <div class="flex items-center gap-3 pt-2">
                                      {
                                          let vid = item.variant_id.clone();
                                          let minus_qty = (item.quantity - 1).to_string();
                                          <form
                                              method="post"
                                              action=(href!(page))
                                              class="inline-flex items-center gap-1"
                                              @submit=$(async |e: Event| {
                                                  e.prevent_default();
                                                  let n = set_cart_quantity_proc(vid.to_owned(), minus_qty.to_owned()).await;
                                                  raw!(
                                                      "(() => { const apply = () => { const badge = document.querySelector('[data-cart-count]'); if (!badge) return; badge.textContent = ${n}; badge.hidden = ${n} === '0'; }; apply(); location.reload(); queueMicrotask(apply); setTimeout(apply, 0); })()",
                                                      { let _ = n.clone(); }
                                                  );
                                              })
                                          >
                                              <input type="hidden" name="variant_id" value=(item.variant_id.clone())>
                                              <input type="hidden" name="quantity" value=(item.quantity - 1)>
                                              button(
                                                  variant: ButtonVariant::Outline,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! {
                                                      type="submit"
                                                      disabled=(item.quantity <= 1)
                                                      aria-label="Diminuir"
                                                  },
                                                  "−"
                                              )
                                          </form>
                                      }
                                      <span class="min-w-8 text-center text-sm">(item.quantity)</span>
                                      {
                                          let vid = item.variant_id.clone();
                                          let plus_qty = (item.quantity + 1).to_string();
                                          <form
                                              method="post"
                                              action=(href!(page))
                                              class="inline-flex items-center gap-1"
                                              @submit=$(async |e: Event| {
                                                  e.prevent_default();
                                                  let n = set_cart_quantity_proc(vid.to_owned(), plus_qty.to_owned()).await;
                                                  raw!(
                                                      "(() => { const apply = () => { const badge = document.querySelector('[data-cart-count]'); if (!badge) return; badge.textContent = ${n}; badge.hidden = ${n} === '0'; }; apply(); location.reload(); queueMicrotask(apply); setTimeout(apply, 0); })()",
                                                      { let _ = n.clone(); }
                                                  );
                                              })
                                          >
                                              <input type="hidden" name="variant_id" value=(item.variant_id.clone())>
                                              <input type="hidden" name="quantity" value=(item.quantity + 1)>
                                              button(
                                                  variant: ButtonVariant::Outline,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! {
                                                      type="submit"
                                                      disabled=(item.quantity >= item.max_quantity)
                                                      aria-label="Aumentar"
                                                  },
                                                  "+"
                                              )
                                          </form>
                                      }
                                      {
                                          let vid = item.variant_id.clone();
                                          <form
                                              method="post"
                                              action=(href!(page))
                                              @submit=$(async |e: Event| {
                                                  e.prevent_default();
                                                  let n = remove_from_cart_proc(vid.to_owned()).await;
                                                  raw!(
                                                      "(() => { const apply = () => { const badge = document.querySelector('[data-cart-count]'); if (!badge) return; badge.textContent = ${n}; badge.hidden = ${n} === '0'; }; apply(); location.reload(); queueMicrotask(apply); setTimeout(apply, 0); })()",
                                                      { let _ = n.clone(); }
                                                  );
                                              })
                                          >
                                              <input type="hidden" name="variant_id" value=(item.variant_id.clone())>
                                              <input type="hidden" name="remove" value="true">
                                              button(
                                                  variant: ButtonVariant::Ghost,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" aria-label="Remover" },
                                                  "✕"
                                              )
                                          </form>
                                      }
                                  </div>
                              </div>
                              <p class="shrink-0 text-right text-sm font-medium text-primary">
                                  (format_price(item.unit_price_cents * item.quantity))
                              </p>
                          )
                      )
                  }

                  card(
                      card_header(
                          card_title("Resumo do pedido")
                      )
                      card_content(
                          if missing_for_free_shipping > 0 {
                              <p class="text-sm text-muted-foreground">
                                  "Faltam " (format_price(missing_for_free_shipping)) " para frete grátis."
                              </p>
                          } else {
                              <p class="text-sm text-primary">"Você ganhou frete grátis!"</p>
                          }
                          separator(attrs: attributes! { class="my-1" })
                          <div class="flex items-center justify-between">
                              <span class="text-sm text-muted-foreground">"Subtotal"</span>
                              <span class="text-xl font-semibold text-primary">(format_price(subtotal))</span>
                          </div>
                          if discount > 0 {
                              <div class="flex items-center justify-between text-sm">
                                  <span class="text-muted-foreground">"Desconto"</span>
                                  <span>"-" (format_price(discount))</span>
                              </div>
                          }
                          field(
                              field_label(attrs: attributes! { for="coupon" }, "Cupom")
                              <form method="post" action=(href!(page)) class="flex items-center gap-2">
                                  <input type="hidden" name="intent" value="apply_coupon">
                                  input(attrs: attributes! {
                                      id="coupon"
                                      name="coupon_code"
                                      placeholder="CUPOM10"
                                      class="min-w-0 flex-1"
                                      value=(coupon_input.clone())
                                  })
                                  button(
                                      variant: ButtonVariant::Outline,
                                      attrs: attributes! { type="submit" },
                                      "Aplicar"
                                  )
                              </form>
                              if resolved_coupon.is_some() {
                                  <form method="post" action=(href!(page)) class="mt-2">
                                      <input type="hidden" name="intent" value="remove_coupon">
                                      button(
                                          variant: ButtonVariant::Ghost,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! { type="submit" },
                                          "Remover cupom"
                                      )
                                  </form>
                              }
                              if let Some(ref message) = coupon_error {
                                  field_error((message.as_str()))
                              }
                          )
                          if let Some(ref coupon) = resolved_coupon {
                              <p class="text-sm text-muted-foreground">
                                  "Cupom " (coupon.code.as_str()) " aplicado."
                              </p>
                          }
                          <p class="text-xs text-muted-foreground">
                              "Frete grátis para compras acima de R$ 299. Calculado no checkout."
                          </p>
                      )
                      card_footer(
                          <div class="flex w-full flex-col gap-3 @sm/page:flex-row">
                              <a
                                  href=(href!(crate::app::produtos::page))
                                  class=(button_variants(ButtonVariant::Outline, ButtonSize::Lg))
                              >
                                  "Continuar comprando"
                              </a>
                              <a
                                  href=(href!(crate::app::checkout::page))
                                  class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                              >
                                  "Finalizar compra"
                              </a>
                          </div>
                      )
                  )
              </div>
          }
      )
  })
}
