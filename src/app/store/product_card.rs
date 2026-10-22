use topcoat::{
  Result,
  context::Cx,
  icon::{icon, iconify::iconify_icon},
  router::href,
  view::{View, attributes, component, view},
};

use crate::components::badge::{BadgeVariant, badge};

use super::pricing::{discount_percent, pix_price, storefront_settings};
use super::queries::{ProductSummary, category_label, format_price};
use super::reviews::card_rating;
use super::wishlist::{liked_product_ids, wishlist_button};

#[component]
pub async fn product_card(
  cx: &Cx,
  product: ProductSummary,
) -> Result<impl View> {
  let sold_out = product.available_total == 0;
  let image_url = product.image_url.clone();
  let secondary_image_url = product.secondary_image_url.clone();
  let name = product.name.clone();
  let category = category_label(&product.category).to_string();
  let price = format_price(product.price_in_cents);
  let url = href!(
    crate::app::produtos::slug::page,
    crate::app::produtos::slug::Slug(product.slug.clone())
  );

  let settings = storefront_settings(cx).await;
  let pix_discount_percent = settings.pix_discount_percent;
  let pix_price_cents = if pix_discount_percent > 0 {
    pix_price(product.price_in_cents, pix_discount_percent)
  } else {
    0
  };
  let discount_badge_text =
    discount_percent(product.compare_at_price_in_cents, product.price_in_cents)
      .map(|pct| format!("-{pct}%"));

  let liked = liked_product_ids(cx).await.contains(&product.id);
  let product_id = product.id.to_string();

  Ok(view! {
      <article class="group relative">
          <a href=(url) class="block">
              <div class="relative overflow-hidden bg-foreground/5 rounded-lg">
                  <div class="aspect-[3/4] overflow-hidden relative">
                      if let Some(main_url) = image_url {
                          <img
                              src=(main_url)
                              alt=(name.clone())
                              class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-[1.04]"
                          >
                          if let Some(secondary_url) = secondary_image_url {
                              <img
                                  src=(secondary_url)
                                  alt=(name.clone())
                                  class="absolute inset-0 h-full w-full object-cover opacity-0 transition-opacity duration-700 group-hover:opacity-100"
                              >
                          }
                      } else {
                          <div class="h-full w-full bg-foreground/5 flex items-center justify-center">
                              icon(data: iconify_icon!("lucide:image"))
                          </div>
                      }
                  </div>

                  if let Some(discount_text) = discount_badge_text {
                      badge(
                          variant: BadgeVariant::Primary,
                          attrs: attributes! { class="absolute left-4 top-4" },
                          (discount_text)
                      )
                  }

                  if sold_out {
                      badge(
                          variant: BadgeVariant::Primary,
                          attrs: attributes! { class="absolute bottom-4 left-4" },
                          "Esgotado"
                      )
                  }
              </div>
              <div class="space-y-2 pt-4">
                  <p class="text-[10px] tracking-wide uppercase text-muted-foreground">(category)</p>
                  <h3 class="font-serif text-xl leading-tight text-foreground">(product.name.clone())</h3>
                  card_rating(product_id: product.id)

                  if !product.colors.is_empty() {
                      <div class="flex gap-2 items-center">
                          for (color_name, color_hex) in product.colors.iter().take(5) {
                              if let Some(hex) = color_hex {
                                  <div
                                      class="w-4 h-4 rounded-full border-2 border-border"
                                      style=(format!("background-color: {hex}"))
                                      title=(color_name)
                                  ></div>
                              } else {
                                  <div
                                      class="w-4 h-4 rounded-full border-2 border-border bg-foreground/10"
                                      title=(color_name)
                                  ></div>
                              }
                          }
                          if product.colors.len() > 5 {
                              <span class="text-xs text-muted-foreground">
                                  (format!("+{}", product.colors.len() - 5))
                              </span>
                          }
                      </div>
                  }

                  <div class="space-y-1">
                      if let Some(compare_at) = product.compare_at_price_in_cents {
                          <p class="text-xs text-muted-foreground line-through">
                              (format_price(compare_at))
                          </p>
                      }
                      if pix_discount_percent > 0 {
                          <p class="text-sm font-bold text-primary">
                              (format!("{} no Pix", format_price(pix_price_cents)))
                          </p>
                          <p class="text-xs text-muted-foreground">
                              "ou " (price)
                          </p>
                      } else {
                          <p class="text-sm font-medium text-primary">(price)</p>
                      }
                  </div>
              </div>
          </a>

          <div class="absolute right-4 top-4 z-10">
              wishlist_button(product_id: product_id, liked: liked, next: String::new())
          </div>
      </article>
  })
}
