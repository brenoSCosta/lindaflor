pub mod slug;

use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page},
  view::{View, attributes, view},
};

use super::store::queries::{format_price, list_sets};
use crate::app::utils::{object_store, resolve_storage_url};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::card::{
  card, card_description, card_footer, card_header, card_title,
};
use crate::components::container::container;

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let sets = list_sets(pool).await?;

  let mut covers = Vec::with_capacity(sets.len());
  for set in &sets {
    covers.push(resolve_storage_url(&store, set.cover_url.as_deref()).await);
  }

  Ok(view! {
      container(
          <h1 class="text-4xl font-bold tracking-tight">"Conjuntos"</h1>
          <p class="max-w-xl text-muted-foreground">
              "Combine top e calcinha em conjuntos prontos, com preço especial."
          </p>
          if sets.is_empty() {
              <p class="text-muted-foreground">
                  "Nenhum conjunto disponível no momento. Volte em breve."
              </p>
          } else {
              <div class="grid gap-4 @md/page:grid-cols-2">
                  #[key(set.id.to_string())]
                  for (set, cover) in sets.iter().zip(covers.iter()) {
                      let cover_url = cover
                          .clone()
                          .unwrap_or_else(|| "/static/product-fallback.svg".to_string());
                      let name = set.name.clone();
                      let description = set.description.clone();
                      let product_count = set.product_count;
                      let price = set.price_in_cents;
                      <a
                          href=(href!(slug::page, slug::Slug(set.slug.clone())))
                          class="block transition-colors"
                      >
                          card(
                              attrs: attributes! { class="overflow-hidden hover:border-primary" },
                              <div class="aspect-[16/9] overflow-hidden bg-foreground/5">
                                  <img
                                      src=(cover_url)
                                      alt=(name.clone())
                                      class="h-full w-full object-cover"
                                  >
                              </div>
                              card_header(
                                  card_title((name))
                                  if let Some(description) = description {
                                      card_description((description))
                                  }
                              )
                              card_footer(
                                  badge(variant: BadgeVariant::Secondary, (product_count) " peças")
                                  if let Some(price) = price {
                                      <span class="text-sm font-medium">(format_price(price))</span>
                                  }
                              )
                          )
                      </a>
                  }
              </div>
          }
      )
  })
}
