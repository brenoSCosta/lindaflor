pub mod slug;

use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::page,
  view::{View, attributes, view},
};

use super::store::queries::list_collections;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::card::{
  card, card_description, card_footer, card_header, card_title,
};

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let collections = list_collections(pool).await?;

  Ok(view! {
      <main class="mx-auto max-w-7xl px-4 py-16 md:px-8">
          <h1 class="text-4xl font-bold tracking-tight">"Coleções"</h1>
          <p class="mt-3 max-w-xl text-muted-foreground">
              "Descubra as linhas da Linda Flor, pensadas para cada momento do seu verão."
          </p>
          <div class="mt-12 grid gap-6 md:grid-cols-2">
              #[key(collection.id.to_string())]
              for collection in collections {
                  <a
                      href=(format!("/colecoes/{}", collection.slug))
                      class="block transition-colors"
                  >
                      card(
                          attrs: attributes! { class="hover:border-primary" },
                          card_header(
                              card_title((collection.name.clone()))
                              if let Some(description) = collection.description.clone() {
                                  card_description((description))
                              }
                          )
                          card_footer(
                              badge(variant: BadgeVariant::Secondary, (collection.product_count) " peças")
                          )
                      )
                  </a>
              }
          </div>
      </main>
  })
}
