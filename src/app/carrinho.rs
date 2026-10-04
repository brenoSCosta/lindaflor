use serde::Deserialize;
use topcoat::{
  Result,
  context::Cx,
  router::{content::Form, href, page},
  view::{View, attributes, view},
};

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
  cart_item_count, cart_subtotal_cents, read_cart, remove_item, update_quantity,
};
use crate::app::store::queries::format_price;

#[derive(Deserialize)]
pub struct CartUpdateInput {
  variant_id: Option<String>,
  quantity: Option<i32>,
  remove: Option<String>,
}

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<CartUpdateInput>>,
) -> Result<impl View> {
  if let Some(Form(input)) = body
    && let Some(variant_id) =
      input.variant_id.as_deref().filter(|v| !v.is_empty())
  {
    if input.remove.as_deref() == Some("true") {
      remove_item(cx, variant_id);
    } else if let Some(quantity) = input.quantity {
      update_quantity(cx, variant_id, quantity);
    }
  }

  let items = read_cart(cx);
  let subtotal = cart_subtotal_cents(&items);
  let item_count = cart_item_count(&items);
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
                                      <form method="post" action=(href!(page)) class="inline-flex items-center gap-1">
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
                                      <span class="min-w-8 text-center text-sm">(item.quantity)</span>
                                      <form method="post" action=(href!(page)) class="inline-flex items-center gap-1">
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
                                      <form method="post" action=(href!(page))>
                                          <input type="hidden" name="variant_id" value=(item.variant_id.clone())>
                                          <input type="hidden" name="remove" value="true">
                                          button(
                                              variant: ButtonVariant::Ghost,
                                              size: ButtonSize::Sm,
                                              attrs: attributes! { type="submit" aria-label="Remover" },
                                              "✕"
                                          )
                                      </form>
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
