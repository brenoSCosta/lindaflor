use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::href,
  runtime::{Event, Signal, shard, signal},
  view::{View, attributes, view},
};

use crate::auth::user::current_user_owned;
use crate::components::alert::{AlertVariant, alert, alert_title};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::dialog::{dialog_header, dialog_title};
use crate::components::field::{field, field_label};
use crate::components::input::input;
use crate::components::separator::separator;

use crate::app::store::cart::{
  cart_coupon_code, cart_subtotal_cents, hydrate_cart, load_or_create_cart,
  remove_from_cart_proc, set_cart_coupon_proc, set_cart_quantity_proc,
};
use crate::app::store::coupons::{CouponReject, resolve_coupon};
use crate::app::store::queries::format_price;

/// NOTE for the badge-patch `raw!` calls below: the JS must end with `;`.
/// `raw!` output is spliced into the handler without a separator, so
/// without it the next statement fuses into a call on the IIFE result
/// (`})()(...))`) and throws before `version.set` runs.
///
/// Cart drawer body. Shard-only: every mutation goes through a `procedure`
/// and bumps `version` to re-render this shard in place — no form POSTs,
/// no `location.reload()`.
///
/// Coupon errors are browser-only: `coupon_error` is only read inside
/// `$(...)`, so setting it patches the DOM without a shard request.
/// Success clears the error and bumps `version` for fresh totals.
#[shard("/carrinho/sheet")]
pub async fn cart_sheet(cx: &Cx, open: Signal<bool>) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let cart = load_or_create_cart(cx, pool, user_id).await?;

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
  let coupon_initial = stored_coupon.clone().unwrap_or_default();

  let mut preview_tx = pool.begin().await?;
  let coupon_outcome =
    resolve_coupon(&mut preview_tx, &coupon_initial, subtotal, user_id, false)
      .await;
  preview_tx.rollback().await?;
  let (resolved_coupon, preview_error) = match coupon_outcome {
    Ok(coupon) => (coupon, None),
    Err(CouponReject::Db(error)) => return Err(error.into()),
    Err(CouponReject::Invalid(message)) => (None, Some(message)),
  };
  let discount = resolved_coupon
    .as_ref()
    .map(|coupon| coupon.discount_cents)
    .unwrap_or(0);
  let item_count: i32 = items.iter().map(|item| item.quantity).sum();
  let free_shipping_threshold = 29900;
  let missing_for_free_shipping = (free_shipping_threshold - subtotal).max(0);

  // Server-tracked: bumping `version` re-renders this shard, and any
  // change to `open` (owned by the shell) re-renders it too — so items
  // added elsewhere (e.g. product page) are fresh every time the drawer
  // opens. Writes like `open.set(false)` inside `$(...)` stay
  // browser-only and never trigger a render on their own.
  let version = signal(cx, || 0u64);
  let _tick = version.get();
  let _open_tick = open.get();
  // Browser-only: only read inside `$(...)`, never `.get()` on the server,
  // so typing and inline errors never trigger a shard request.
  let coupon_input = signal(cx, || coupon_initial);
  let coupon_error = signal(cx, || preview_error.clone().unwrap_or_default());

  Ok(view! {
      <div class="flex h-full min-h-0 flex-col gap-4">
          dialog_header(
              attrs: attributes! { class="shrink-0" },
              <div class="flex items-start justify-between gap-4">
                  <div>
                      dialog_title("Carrinho")
                      if item_count > 0 {
                          <p class="mt-1 text-sm text-muted-foreground">
                              (item_count)
                              " "
                              if item_count == 1 {
                                  "peça"
                              } else {
                                  "peças"
                              }
                          </p>
                      }
                  </div>
                  <div class="flex items-center gap-2">
                      if item_count > 0 {
                          badge(variant: BadgeVariant::Secondary, (item_count))
                      }
                      button(
                          variant: ButtonVariant::Ghost,
                          size: ButtonSize::Icon,
                          attrs: attributes! {
                              type="button"
                              aria-label="Fechar carrinho"
                              @click=$(|_e: Event| open.set(false))
                          },
                          "✕"
                      )
                  </div>
              </div>
          )

          if !notices.is_empty() {
              <div class="flex shrink-0 flex-col gap-2">
                  for notice in notices {
                      alert(
                          variant: AlertVariant::Neutral,
                          alert_title((notice.message()))
                      )
                  }
              </div>
          }

          if items.is_empty() {
              <div class="flex flex-1 flex-col justify-center py-10 text-center">
                  <p class="text-muted-foreground">"Seu carrinho está vazio."</p>
                  <div class="mt-4 flex justify-center gap-2">
                      <a
                          href=(href!(crate::app::produtos::page))
                          class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
                      >
                          "Ver catálogo"
                      </a>
                      button(
                          variant: ButtonVariant::Outline,
                          size: ButtonSize::Md,
                          attrs: attributes! { type="button" @click=$(|_e: Event| open.set(false)) },
                          "Fechar"
                      )
                  </div>
              </div>
          } else {
              <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
                  #[key(item.variant_id.clone())]
                  for item in items {
                      <div
                          class="flex items-center gap-3 rounded-lg border border-border p-3"
                      >
                          <a
                              href=(href!(
                                  crate::app::produtos::slug::page,
                                  crate::app::produtos::slug::Slug(item.product_slug.clone()),
                              ))
                              class="shrink-0 overflow-hidden rounded-md"
                          >
                              if let Some(image_url) = &item.image_url {
                                  <img
                                      src=(image_url.clone())
                                      alt=(item.product_name.clone())
                                      class="h-20 w-16 object-cover"
                                  >
                              }
                          </a>
                          <div class="min-w-0 flex-1 space-y-1">
                              <a
                                  href=(href!(
                                      crate::app::produtos::slug::page,
                                      crate::app::produtos::slug::Slug(item.product_slug.clone()),
                                  ))
                                  class="block truncate text-sm font-medium uppercase tracking-wide transition-colors hover:text-primary"
                              >
                                  (item.product_name)
                              </a>
                              <p class="text-xs text-muted-foreground">
                                  (item.variant_label)
                              </p>
                              <div class="flex items-center gap-2 pt-1">
                                  {
                                      let vid = item.variant_id.clone();
                                      let minus_qty = (item.quantity - 1).to_string();
                                      button(
                                          variant: ButtonVariant::Outline,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! {
                                              type="button"
                                              disabled=(item.quantity <= 1)
                                              aria-label="Diminuir"
                                              @click=$(async |_e: Event| {
                                                  let n = set_cart_quantity_proc(
                                                      vid.to_owned(),
                                                      minus_qty.to_owned(),
                                                  ).await;
                                                  raw!(
                                                      "(() => { const badge = document.querySelector('[data-cart-count]'); if (!badge) return; badge.textContent = ${n}; badge.hidden = ${n} === '0'; })();",
                                                      {
                                                          let _ = n.clone();
                                                      },
                                                  );
                                                  version.set(version.get() + 1u64);
                                              })
                                          },
                                          "-"
                                      )
                                  }
                                  <span class="min-w-8 text-center text-sm">
                                      (item.quantity)
                                  </span>
                                  {
                                      let vid = item.variant_id.clone();
                                      let plus_qty = (item.quantity + 1).to_string();
                                      button(
                                          variant: ButtonVariant::Outline,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! {
                                              type="button"
                                              disabled=(item.quantity >= item.max_quantity)
                                              aria-label="Aumentar"
                                              @click=$(async |_e: Event| {
                                                  let n = set_cart_quantity_proc(
                                                      vid.to_owned(),
                                                      plus_qty.to_owned(),
                                                  ).await;
                                                  raw!(
                                                      "(() => { const badge = document.querySelector('[data-cart-count]'); if (!badge) return; badge.textContent = ${n}; badge.hidden = ${n} === '0'; })();",
                                                      {
                                                          let _ = n.clone();
                                                      },
                                                  );
                                                  version.set(version.get() + 1u64);
                                              })
                                          },
                                          "+"
                                      )
                                  }
                                  {
                                      let vid = item.variant_id.clone();
                                      button(
                                          variant: ButtonVariant::Ghost,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! {
                                              type="button"
                                              aria-label="Remover"
                                              @click=$(async |_e: Event| {
                                                  let n = remove_from_cart_proc(vid.to_owned()).await;
                                                  raw!(
                                                      "(() => { const badge = document.querySelector('[data-cart-count]'); if (!badge) return; badge.textContent = ${n}; badge.hidden = ${n} === '0'; })();",
                                                      {
                                                          let _ = n.clone();
                                                      },
                                                  );
                                                  version.set(version.get() + 1u64);
                                              })
                                          },
                                          "✕"
                                      )
                                  }
                              </div>
                          </div>
                          <p class="shrink-0 text-right text-sm font-medium text-primary">
                              (format_price(item.unit_price_cents * item.quantity))
                          </p>
                      </div>
                  }
              </div>

              <div class="flex shrink-0 flex-col gap-3 rounded-lg border border-border p-4">
                  if missing_for_free_shipping > 0 {
                      <p class="text-sm text-muted-foreground">
                          "Faltam "
                          (format_price(missing_for_free_shipping))
                          " para frete grátis."
                      </p>
                  } else {
                      <p class="text-sm text-primary">"Você ganhou frete grátis!"</p>
                  }
                  separator(attrs: attributes! { class="my-1" })
                  <div class="flex items-center justify-between">
                      <span class="text-sm text-muted-foreground">"Subtotal"</span>
                      <span class="text-lg font-semibold text-primary">
                          (format_price(subtotal))
                      </span>
                  </div>
                  if discount > 0 {
                      <div class="flex items-center justify-between text-sm">
                          <span class="text-muted-foreground">"Desconto"</span>
                          <span>
                              "-"
                              (format_price(discount))
                          </span>
                      </div>
                  }
                  field(
                      field_label(attrs: attributes! { for="cart-coupon" }, "Cupom")
                      <div class="flex items-center gap-2">
                          input(
                              value: coupon_input.clone(),
                              attrs: attributes! {
                                  id="cart-coupon"
                                  placeholder="CUPOM10"
                                  class="min-w-0 flex-1"
                                  autocomplete="off"
                              }
                          )
                          button(
                              variant: ButtonVariant::Outline,
                              attrs: attributes! {
                                  type="button"
                                  @click=$(async |_e: Event| {
                                      let message = set_cart_coupon_proc(coupon_input.get()).await;
                                      if message.is_empty() {
                                          coupon_error.set("".to_owned());
                                          version.set(version.get() + 1u64);
                                      } else {
                                          coupon_error.set(message);
                                      }
                                  })
                              },
                              "Aplicar"
                          )
                      </div>
                      <p
                          class="text-sm text-destructive"
                          :hidden=$(coupon_error.get().is_empty())
                      >
                          $(coupon_error.get())
                      </p>
                  )
                  if let Some(ref coupon) = resolved_coupon {
                      <div class="flex items-center justify-between gap-2">
                          <p class="text-sm text-muted-foreground">
                              "Cupom "
                              (coupon.code.as_str())
                              " aplicado."
                          </p>
                          button(
                              variant: ButtonVariant::Ghost,
                              size: ButtonSize::Sm,
                              attrs: attributes! {
                                  type="button"
                                  @click=$(async |_e: Event| {
                                      let message = set_cart_coupon_proc("".to_owned()).await;
                                      if message.is_empty() {
                                          coupon_input.set("".to_owned());
                                          coupon_error.set("".to_owned());
                                          version.set(version.get() + 1u64);
                                      } else {
                                          coupon_error.set(message);
                                      }
                                  })
                              },
                              "Remover"
                          )
                      </div>
                  }
                  <p class="text-xs text-muted-foreground">
                      "Frete grátis para compras acima de R$ 299. Calculado no checkout."
                  </p>
              </div>

              <div class="flex shrink-0 flex-col gap-2">
                  <a
                      href=(href!(crate::app::checkout::page))
                      class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                  >
                      "Finalizar compra"
                  </a>
                  button(
                      variant: ButtonVariant::Outline,
                      size: ButtonSize::Lg,
                      attrs: attributes! {
                          type="button"
                          class="w-full"
                          @click=$(|_e: Event| open.set(false))
                      },
                      "Continuar comprando"
                  )
              </div>
          }
      </div>
  })
}
