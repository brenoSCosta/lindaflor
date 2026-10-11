use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  view::{View, component, view},
};
use uuid::Uuid;

use crate::app::utils::{object_store, resolve_storage_url};

const MAX_BANNERS: i64 = 5;

type SlideDbRow = (
  Uuid,
  String,
  Option<String>,
  Option<String>,
  Option<String>,
  String,
);

struct Slide {
  id: String,
  title: String,
  subtitle: Option<String>,
  cta_label: String,
  link_url: String,
  image_url: String,
}

/// Home hero: active `home_banners` as a scroll-snap carousel. Renders
/// nothing when there are no banners so the caller keeps its static hero.
#[component]
pub async fn hero_banners(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);

  let rows: Vec<SlideDbRow> = sqlx::query_as(
    "SELECT id, title, subtitle, cta_label, link_url, image_url
       FROM home_banners WHERE active = true
       ORDER BY sort_order ASC, created_at DESC LIMIT $1",
  )
  .bind(MAX_BANNERS)
  .fetch_all(pool)
  .await?;

  let mut slides = Vec::with_capacity(rows.len());
  for (id, title, subtitle, cta_label, link_url, image_url) in rows {
    let Some(image_url) = resolve_storage_url(&store, Some(&image_url)).await
    else {
      continue;
    };
    slides.push(Slide {
      id: id.to_string(),
      title,
      subtitle,
      cta_label: cta_label.unwrap_or_else(|| "Ver coleção".to_string()),
      link_url: link_url.unwrap_or_else(|| "/produtos".to_string()),
      image_url,
    });
  }
  let multiple = slides.len() > 1;

  Ok(view! {
      if !slides.is_empty() {
          <section class="relative" aria-label="Destaques">
              <div class="flex snap-x snap-mandatory overflow-x-auto scroll-smooth [scrollbar-width:none]">
                  #[key(slide.id.clone())]
                  for slide in slides {
                      <a href=(slide.link_url.clone()) class="relative block h-[70vh] min-h-[420px] w-full shrink-0 snap-center overflow-hidden">
                          <img src=(slide.image_url.clone()) alt=(slide.title.clone()) class="absolute inset-0 h-full w-full object-cover">
                          <div class="absolute inset-0 bg-gradient-to-t from-black/50 via-black/10 to-transparent"></div>
                          <div class="absolute inset-x-0 bottom-0 p-8 text-white md:p-16">
                              <h2 class="font-serif text-4xl leading-tight md:text-6xl">(slide.title.clone())</h2>
                              if let Some(subtitle) = slide.subtitle.clone() {
                                  <p class="mt-3 max-w-xl text-base md:text-lg">(subtitle)</p>
                              }
                              <span class="mt-6 inline-block border border-white px-6 py-3 text-xs tracking-[0.2em] uppercase">(slide.cta_label.clone())</span>
                          </div>
                      </a>
                  }
              </div>
              if multiple {
                  <p class="pointer-events-none absolute right-6 top-6 text-[10px] tracking-[0.2em] uppercase text-white/80">"Deslize →"</p>
              }
          </section>
      }
  })
}
