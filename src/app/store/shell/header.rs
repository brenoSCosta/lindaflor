use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  icon::{icon, iconify::iconify_icon},
  router::href,
  runtime::Signal,
  view::{View, attributes, class, component, view},
};

use crate::app::auth_helpers::optional_user;
use crate::app::store::queries::list_collections;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};

#[component]
pub async fn header(
  cx: &Cx,
  cart_count: usize,
  cart_open: Signal<bool>,
) -> Result<impl View> {
  let logged_in = optional_user(cx).await?.is_some();
  let pool = app_context::<PgPool>(cx);

  // Fetch collections (up to 6 will be shown in dropdown)
  let collections = list_collections(pool)
    .await?
    .into_iter()
    .take(6)
    .collect::<Vec<_>>();

  Ok(view! {
      <header class="sticky top-0 z-50 border-b border-border bg-background backdrop-blur">
          <div class="relative mx-auto max-w-7xl px-4 md:px-8">
            <div class="flex items-center justify-between gap-4 h-20">
              // Mobile menu toggle (no JS: <details> opens the panel below)
              <details class="group/menu md:hidden">
                <summary
                  class=(class!(button_variants(ButtonVariant::Ghost, ButtonSize::Icon), "list-none [&::-webkit-details-marker]:hidden"))
                  aria-label="Menu"
                  title="Menu"
                >
                  icon(data: iconify_icon!("lucide:menu"))
                </summary>
                <nav class="absolute inset-x-0 top-full border-b border-border bg-background px-4 py-4 shadow-lg">
                  <div class="flex flex-col gap-1 text-sm">
                    <a href=(href!(crate::app::produtos::page).query([("sort", "novos")])) class="py-2">"Novidades"</a>
                    <p class="pt-2 text-[10px] tracking-widest uppercase text-muted-foreground">"Biquínis"</p>
                    <a href=(href!(crate::app::produtos::page).query([("category", "top_biquini")])) class="py-2 pl-3">"Tops"</a>
                    <a href=(href!(crate::app::produtos::page).query([("category", "calcinha_biquini")])) class="py-2 pl-3">"Calcinhas"</a>
                    <a href=(href!(crate::app::produtos::page).query([("category", "biquini")])) class="py-2 pl-3">"Conjuntos"</a>
                    <a href=(href!(crate::app::produtos::page).query([("category", "maio")])) class="py-2">"Maiôs"</a>
                    <a href=(href!(crate::app::produtos::page).query([("category", "saida_praia")])) class="py-2">"Saídas de praia"</a>
                    <a href=(href!(crate::app::produtos::page).query([("category", "acessorio")])) class="py-2">"Acessórios"</a>
                    <a href=(href!(crate::app::colecoes::page)) class="py-2">"Coleções"</a>
                    <a href=(href!(crate::app::produtos::page).query([("sale", "true")])) class="py-2 font-semibold text-red-600">"Liquida"</a>
                  </div>
                </nav>
              </details>

              // Logo (center on mobile, left on desktop)
              <a href=(href!(crate::app::page)) class="text-left md:flex-1 md:order-2 md:text-center">
                <span class="font-serif text-2xl leading-none text-foreground">"Linda Flor"</span>
                <span class="block text-[10px] tracking-[0.24em] text-primary uppercase">"Moda Praia"</span>
              </a>

              // Desktop Navigation (hidden on mobile, visible on md and up)
              <nav class="hidden items-center gap-6 md:flex md:order-1 md:flex-1">
                <a href=(href!(crate::app::produtos::page).query([("sort", "novos")])) class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">
                  "Novidades"
                </a>

                // Biquínis dropdown
                <div class="group relative">
                  <button
                    class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary flex items-center gap-1"
                    type="button"
                    aria-expanded="false"
                  >
                    "Biquínis"
                    icon(data: iconify_icon!("lucide:chevron-down"), attrs: attributes! { class="size-3" })
                  </button>
                  <div class="absolute left-0 top-full hidden pt-2 group-hover:block group-focus-within:block bg-background border border-border rounded-lg shadow-lg min-w-[200px]">
                    <a href=(href!(crate::app::produtos::page).query([("category", "top_biquini")])) class="block px-4 py-2 text-sm hover:bg-foreground/5 hover:text-primary">
                      "Tops"
                    </a>
                    <a href=(href!(crate::app::produtos::page).query([("category", "calcinha_biquini")])) class="block px-4 py-2 text-sm hover:bg-foreground/5 hover:text-primary">
                      "Calcinhas"
                    </a>
                    <a href=(href!(crate::app::produtos::page).query([("category", "biquini")])) class="block px-4 py-2 text-sm hover:bg-foreground/5 hover:text-primary">
                      "Conjuntos"
                    </a>
                  </div>
                </div>

                <a href=(href!(crate::app::produtos::page).query([("category", "maio")])) class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">
                  "Maiôs"
                </a>

                <a href=(href!(crate::app::produtos::page).query([("category", "saida_praia")])) class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">
                  "Saídas de praia"
                </a>

                <a href=(href!(crate::app::produtos::page).query([("category", "acessorio")])) class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary">
                  "Acessórios"
                </a>

                // Coleções dropdown
                <div class="group relative">
                  <button
                    class="text-[10px] tracking-wider uppercase transition-colors hover:text-primary flex items-center gap-1"
                    type="button"
                    aria-expanded="false"
                  >
                    "Coleções"
                    icon(data: iconify_icon!("lucide:chevron-down"), attrs: attributes! { class="size-3" })
                  </button>
                  <div class="absolute left-0 top-full hidden pt-2 group-hover:block group-focus-within:block bg-background border border-border rounded-lg shadow-lg min-w-[200px]">
                    #[key(col.slug.clone())]
                    for col in collections.iter() {
                      <a href=(href!(crate::app::colecoes::slug::page, crate::app::colecoes::slug::Slug(col.slug.clone()))) class="block px-4 py-2 text-sm hover:bg-foreground/5 hover:text-primary">
                        (col.name.as_str())
                      </a>
                    }
                    <div class="border-t border-border"></div>
                    <a href=(href!(crate::app::colecoes::page)) class="block px-4 py-2 text-sm font-semibold hover:bg-foreground/5 hover:text-primary">
                      "Ver todas"
                    </a>
                  </div>
                </div>

                <a href=(href!(crate::app::produtos::page).query([("sale", "true")])) class="text-[10px] tracking-wider uppercase font-semibold text-red-600 transition-colors hover:text-red-700">
                  "Liquida"
                </a>
              </nav>

              // Right icons section
              <div class="flex items-center justify-end gap-2 md:order-3 md:flex-1">
                // Search
                <a
                  href=(href!(crate::app::produtos::page))
                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Icon))
                  aria-label="Buscar"
                  title="Buscar"
                >
                  icon(data: iconify_icon!("lucide:search"))
                </a>

                // Favorites
                <a
                  href=(href!(crate::app::favoritos::page))
                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Icon))
                  aria-label="Favoritos"
                  title="Favoritos"
                >
                  icon(data: iconify_icon!("lucide:heart"))
                </a>

                // Account
                if logged_in {
                  <a
                    href=(href!(crate::app::dashboard::page))
                    class=(button_variants(ButtonVariant::Ghost, ButtonSize::Icon))
                    aria-label="Conta"
                    title="Conta"
                  >
                    icon(data: iconify_icon!("lucide:user"))
                  </a>
                } else {
                  <a
                    href=(href!(crate::app::login::page))
                    class=(button_variants(ButtonVariant::Ghost, ButtonSize::Icon))
                    aria-label="Entrar"
                    title="Entrar"
                  >
                    icon(data: iconify_icon!("lucide:user"))
                  </a>
                }

                // Cart
                <button
                  type="button"
                  id="cart-open-btn"
                  data-cart-open=""
                  class=(class!(button_variants(ButtonVariant::Ghost, ButtonSize::Icon), "relative"))
                  aria-label="Carrinho"
                  title="Carrinho"
                  @click=$(|_e: topcoat::runtime::Event| cart_open.set(true))
                >
                  icon(
                    data: iconify_icon!("lucide:shopping-cart")
                  )
                  <span
                    data-cart-count=""
                    class="pointer-events-none absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-red-500 px-1 text-[10px] leading-none font-semibold text-white"
                    hidden=(cart_count == 0)
                  >
                    (cart_count)
                  </span>
                </button>
              </div>
            </div>
          </div>
      </header>
  })
}
