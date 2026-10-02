use topcoat::{
  Result,
  view::{View, attributes, component, view},
};

use crate::components::badge::{BadgeVariant, badge};

use super::queries::{ProductSummary, category_label, format_price};

#[component]
pub async fn product_card(product: ProductSummary) -> Result<impl View> {
  let sold_out = product.available_total == 0;
  let image_url = product
    .image_url
    .clone()
    .unwrap_or_else(|| "/static/product-fallback.svg".to_string());
  let name = product.name.clone();
  let category = category_label(&product.category).to_string();
  let price = format_price(product.price_in_cents);
  let url = format!("/produtos/{}", product.slug);

  Ok(view! {
      <article class="group">
          <a href=(url) class="block">
              <div class="relative overflow-hidden bg-foreground/5">
                  <div class="aspect-[3/4] overflow-hidden">
                      <img
                          src=(image_url)
                          alt=(name)
                          class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-[1.04]"
                      >
                  </div>
                  if sold_out {
                      badge(
                          variant: BadgeVariant::Primary,
                          attrs: attributes! { class="absolute right-4 top-4" },
                          "Esgotado"
                      )
                  }
              </div>
              <div class="space-y-1 pt-4">
                  <p class="text-[10px] tracking-wide uppercase text-muted-foreground">(category)</p>
                  <h3 class="font-serif text-xl leading-tight text-foreground">(product.name.clone())</h3>
                  <p class="text-sm font-medium text-primary">(price)</p>
              </div>
          </a>
      </article>
  })
}
