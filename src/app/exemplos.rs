use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{
  card, card_description, card_footer, card_header, card_title,
};
use crate::components::separator::separator;
use topcoat::{
  Result,
  router::page,
  view::{View, attributes, view},
};

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      <div class="min-h-screen bg-foreground px-4 py-16 text-background md:px-8">
          <div class="mx-auto max-w-3xl">
              badge(variant: BadgeVariant::Primary, "Design samples")
              <h1 class="mt-3 text-4xl font-bold tracking-tight md:text-5xl">"Três direções para Linda Flor"</h1>
              <p class="mt-4 max-w-2xl text-sm leading-relaxed text-background/65">
                  "Cada exemplo é uma página funcional com dados reais do catálogo. Compare estilos e escolha qual combina mais com a marca."
              </p>
              separator(attrs: attributes! { class="my-8" })
              <div class="flex flex-col gap-4">
                  <a href="/exemplos/editorial" class="transition-colors">
                      card(
                          attrs: attributes! { class="border-background/10! bg-background/5! text-background!" },
                          card_header(
                              <p class="text-xs uppercase tracking-widest text-pink-300">"Homepage"</p>
                              card_title("Editorial Playfair")
                              card_description("Hero full-bleed, tipografia serif grande, logo como wordmark — sem retângulo rosa. Tom tropical e quente.")
                          )
                          card_footer(
                              <span class="text-xs uppercase tracking-widest text-background/40">"MBM Swim · Zimmermann"</span>
                              <span class="ml-auto text-lg text-background/40">"→"</span>
                          )
                      )
                  </a>
                  <a href="/exemplos/bodoni" class="transition-colors">
                      card(
                          attrs: attributes! { class="border-background/10! bg-background/5! text-background!" },
                          card_header(
                              <p class="text-xs uppercase tracking-widest text-pink-300">"Catálogo"</p>
                              card_title("Bodoni Minimal")
                              card_description("Grid tipo revista de moda, filtros laterais, muito whitespace. Visual fashion/luxo.")
                          )
                          card_footer(
                              <span class="text-xs uppercase tracking-widest text-background/40">"Galeria · Boutique"</span>
                              <span class="ml-auto text-lg text-background/40">"→"</span>
                          )
                      )
                  </a>
                  <a href="/exemplos/bossa-nova" class="transition-colors">
                      card(
                          attrs: attributes! { class="border-background/10! bg-background/5! text-background!" },
                          card_header(
                              <p class="text-xs uppercase tracking-widest text-pink-300">"Produto"</p>
                              card_title("Bossa Nova")
                              card_description("PDP comercial e limpa: galeria, tamanhos, CTA rosa, acordeões e cross-sell.")
                          )
                          card_footer(
                              <span class="text-xs uppercase tracking-widest text-background/40">"Solid & Striped · Commerce"</span>
                              <span class="ml-auto text-lg text-background/40">"→"</span>
                          )
                      )
                  </a>
              </div>
              <div class="mt-10">
                  <a
                      href="/"
                      class=(button_variants(
                          ButtonVariant::Outline,
                          ButtonSize::Md,
                      ))
                  >
                      "Voltar ao site atual"
                  </a>
              </div>
          </div>
      </div>
  })
}
