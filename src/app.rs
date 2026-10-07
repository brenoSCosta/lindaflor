pub mod admin;
pub mod app_sidebar;
pub mod auth_helpers;
pub mod carrinho;
pub mod check_email;
pub mod checkout;
pub mod colecoes;
pub mod conta;
pub mod dashboard;
pub mod forgot_password;
pub mod login;
pub mod pedido;
pub mod politica_privacidade;
pub mod produtos;
pub mod reset_password;
pub mod settings;
pub mod store;
pub mod termos;
pub mod trocas_devolucoes;
pub mod two_factor;
pub mod utils;
pub mod verify_email;

use redis::aio::MultiplexedConnection;
use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  asset::{Asset, RouterBuilderAssetExt, asset},
  context::Cx,
  context::app_context,
  cookie::RouterBuilderCookieExt,
  router::{
    BodyLimit, Router, RouterBuilderDiscoverExt, Slot, StatusCode,
    content::Form,
    error::{NotFoundError, SeeOther, see_other},
    href, layout, module_router, not_found, page,
    request::uri,
    route,
  },
  runtime::RouterBuilderRuntimeExt,
  session::RouterBuilderSessionExt,
  tailwind,
  view::{View, ViewExt, class, error_boundary, view},
};

use crate::app::app_sidebar::{app_shell, is_app_shell_path, storefront_shell};
use crate::app::auth_helpers::optional_user;
use crate::auth::service;
use crate::auth::session_config;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::toast::{ToasterOptions, take_toasts, toaster};
use crate::theme::{THEME_INIT_SCRIPT, THEME_TOGGLE_SCRIPT, read_theme};

use self::store::product_card::product_card;
use self::store::queries::list_products;

/// Client-side reveal + `#sobre` landing behavior (see `assets/sobre.js`).
const SOBRE_SCRIPT: Asset = asset!("assets/sobre.js");

pub fn router(
  pool: PgPool,
  valkey: MultiplexedConnection,
  storage: crate::storage::ObjectStore,
) -> Router {
  let mut builder = module_router!()
    .discover()
    .cookies()
    .sessions(session_config())
    .app_context(pool)
    .app_context(valkey)
    .app_context(storage);

  let bundle = topcoat::asset::AssetBundle::load().unwrap_or_else(|err| {
        panic!(
            "failed to load Topcoat asset bundle next to the executable: {err}\n\
             Run `topcoat asset bundle --bin lindaflor` after building (requires DATABASE_URL)."
        )
    });
  builder = builder.assets(bundle);

  builder
    .layer(BodyLimit::max(3 * 1024 * 1024).at("/settings/avatar"))
    // Product create/edit carry product fields plus up to 8 images @ 2MB each.
    .layer(BodyLimit::max(20 * 1024 * 1024).at("/admin/produtos/create"))
    .layer(BodyLimit::max(20 * 1024 * 1024).at("/admin/produtos/update"))
    .runtime()
    .build()
}

#[layout]
async fn root(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
  let theme_class = if read_theme(cx).is_dark() { "dark" } else { "" };
  let toasts = take_toasts(cx);

  let path = uri(cx).path().to_owned();
  let user = optional_user(cx).await?;
  let use_app_shell = user.as_ref().is_some_and(|_| is_app_shell_path(&path));

  let impersonation_name = match &user {
    Some(su) if su.impersonated_by.is_some() => Some(su.user.name.clone()),
    _ => None,
  };

  // Catch-all `not_found!("/")` and handler `ok_or_not_found` both surface as
  // NotFoundError; render a branded page instead of a blank 404 response.
  let guarded = view! {
      error_boundary(
          fallback: |error| {
              if error.downcast_ref::<NotFoundError>().is_some() {
                  Ok(view! {
                      (StatusCode::NOT_FOUND)
                      <main class="mx-auto flex min-h-[70vh] max-w-2xl flex-col items-center justify-center px-4 py-24 text-center md:px-8">
                          <p class="font-serif text-7xl leading-none text-primary md:text-8xl">"404"</p>
                          <h1 class="mt-8 text-2xl font-semibold tracking-tight md:text-3xl">
                              "Página não encontrada"
                          </h1>
                          <p class="mt-4 max-w-md text-sm leading-relaxed text-muted-foreground md:text-base">
                              "O link que você acessou pode estar quebrado, a página ter sido removida ou o endereço não existir."
                          </p>
                          <div class="mt-10 flex flex-wrap items-center justify-center gap-3">
                              <a href=(href!(page)) class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))>
                                  "Voltar ao início"
                              </a>
                              <a href=(href!(crate::app::produtos::page)) class=(button_variants(ButtonVariant::Outline, ButtonSize::Lg))>
                                  "Ver catálogo"
                              </a>
                          </div>
                      </main>
                  })
              } else {
                  Err(error)
              }
          },
          (slot)
      )
  };

  let body = if use_app_shell {
    let su = user.expect("app shell requires authenticated user");
    view! { app_shell(user: su, (guarded)) }.boxed()
  } else {
    view! { storefront_shell((guarded)) }.boxed()
  };

  Ok(view! {
      <!DOCTYPE html>
      <html lang="pt-BR" class=(theme_class)>
          <head>
              <meta charset="UTF-8">
              <meta name="viewport" content="width=device-width, initial-scale=1.0">
              <title>"Linda Flor — Moda Praia"</title>
              <link rel="stylesheet" href=(tailwind::stylesheet!())>
              topcoat::runtime::script()
              topcoat::dev::script()
              <script src=(THEME_INIT_SCRIPT)></script>
              <script src=(THEME_TOGGLE_SCRIPT)></script>
          </head>
          <body>
              if let Some(ref name) = impersonation_name {
                  <div class="flex items-center justify-center gap-4 border-b border-amber-600/40 bg-amber-100 px-4 py-2 text-sm text-amber-950 dark:bg-amber-950/40 dark:text-amber-100">
                      <span>
                          "Você está atuando como "
                          <strong>(name.as_str())</strong>
                      </span>
                      <form method="post" action=(href!(stop_impersonating)) class="inline">
                          <button
                              type="submit"
                              class="rounded border border-amber-700/50 bg-amber-50 px-2 py-0.5 text-xs font-medium text-amber-950 hover:bg-white dark:bg-amber-900 dark:text-amber-50"
                          >
                              "Encerrar"
                          </button>
                      </form>
                  </div>
              }
              toaster(toasts: toasts, options: ToasterOptions::default())
              (body)
          </body>
      </html>
  })
}

#[derive(Deserialize)]
struct StopImpersonatingInput {
  redirect: Option<String>,
}

#[route(POST "/stop-impersonating")]
async fn stop_impersonating(
  cx: &Cx,
  body: Option<Form<StopImpersonatingInput>>,
) -> Result<SeeOther> {
  let pool = app_context::<PgPool>(cx);
  if let Some(su) = optional_user(cx).await?
    && su.impersonated_by.is_some()
  {
    let _ = service::stop_impersonating(cx, pool, &su).await;
  }
  let redirect = body
    .and_then(|Form(b)| b.redirect)
    .filter(|p| p.starts_with('/') && !p.starts_with("//"))
    .unwrap_or_else(|| href!(crate::app::dashboard::page).resolve(cx));
  Ok(see_other(redirect))
}

// Dispatches unmatched URLs through layouts as NotFoundError (bare 404 otherwise).
not_found!("/");

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let store = crate::app::utils::object_store(cx);

  let featured = list_products(pool, None, None, true, true, None).await?;
  let featured =
    crate::app::admin::produtos::resolve_summary_images(&store, featured).await;
  let all_products = list_products(pool, None, None, false, true, None).await?;
  let all_products =
    crate::app::admin::produtos::resolve_summary_images(&store, all_products)
      .await;

  Ok(view! {
      <script src=(SOBRE_SCRIPT) defer=""></script>
      <section class="relative min-h-[85vh] overflow-hidden">
          <img
              src="https://images.unsplash.com/photo-1507525428034-b723cf961d3e?auto=format&fit=crop&w=2000&q=80"
              alt=""
              class="absolute inset-0 h-full w-full object-cover"
          >
          <div class="absolute inset-0 bg-gradient-to-t from-black/55 to-black/10"></div>
          <div class="relative mx-auto flex min-h-[85vh] max-w-7xl items-end px-4 pb-16 md:px-8 md:pb-24">
              <div class="max-w-xl space-y-6 text-white">
                  <p class="text-[10px] tracking-[0.24em] text-white/75 uppercase">"Coleção Verão 2026"</p>
                  <h1 class="font-serif text-5xl leading-tight md:text-6xl">"Elegância tropical"</h1>
                  <p class="max-w-md text-sm leading-relaxed text-white/80 md:text-base">
                      "Peças pensadas para quem ama sol, mar e estilo. Do biquíni clássico à saída de praia perfeita, tudo com a assinatura Linda Flor."
                  </p>
                  <div class="flex flex-wrap gap-3 pt-2">
                      <a href=(href!(crate::app::produtos::page)) class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))>
                          "Comprar agora"
                      </a>
                      <a
                          href=(href!(page).fragment("sobre"))
                          class="inline-flex h-10 items-center justify-center gap-2 rounded-lg border border-white/50 px-5 text-base font-medium text-white transition-colors hover:bg-white/10"
                      >
                          "Ver coleções"
                      </a>
                  </div>
              </div>
          </div>
      </section>

      <section class="mx-auto max-w-7xl px-4 py-20 md:px-8">
          <div class="mb-10">
              <p class="text-[10px] tracking-widest text-primary uppercase">"Categorias"</p>
              <h2 class="font-serif mt-2 text-4xl text-foreground md:text-5xl">"Explore a coleção"</h2>
          </div>
          <div class="grid gap-4 md:grid-cols-3">
              <a href=(href!(crate::app::produtos::page).query([("category", "biquini")])) class="group relative aspect-[4/5] overflow-hidden">
                  <img
                      src="https://images.unsplash.com/photo-1598522325075-6dfac65b8df4?auto=format&fit=crop&w=1200&q=80"
                      alt="Biquínis"
                      class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
                  >
                  <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
                  <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Biquínis"</p>
              </a>
              <a href=(href!(crate::app::produtos::page).query([("category", "maio")])) class="group relative aspect-[4/5] overflow-hidden">
                  <img
                      src="https://images.unsplash.com/photo-1571019613454-1cb2f99b2d8b?auto=format&fit=crop&w=1200&q=80"
                      alt="Maiôs"
                      class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
                  >
                  <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
                  <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Maiôs"</p>
              </a>
              <a href=(href!(crate::app::produtos::page).query([("category", "saida_praia")])) class="group relative aspect-[4/5] overflow-hidden">
                  <img
                      src="https://images.unsplash.com/photo-1519046904884-53103b34b206?auto=format&fit=crop&w=1200&q=80"
                      alt="Saídas de praia"
                      class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
                  >
                  <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
                  <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Saídas de praia"</p>
              </a>
          </div>
      </section>

      <section class="border-y border-border bg-foreground/5 px-4 py-20 md:px-8">
          <div class="mx-auto max-w-7xl">
              <div class="mb-12 flex items-end justify-between gap-6">
                  <div>
                      <p class="text-[10px] tracking-widest text-primary uppercase">"Destaques"</p>
                      <h2 class="font-serif mt-2 text-4xl text-foreground md:text-5xl">"Os favoritos da temporada"</h2>
                  </div>
                  <a href=(href!(crate::app::produtos::page)) class="hidden items-center gap-2 text-[10px] tracking-wider uppercase md:inline-flex">
                      "Ver tudo"
                  </a>
              </div>
              <div class="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
                  #[key(product.id.to_string())]
                  for product in featured {
                      product_card(product: product)
                  }
              </div>
          </div>
      </section>

      <section id="sobre" data-reveal="" class="mx-auto grid max-w-7xl scroll-mt-28 items-center gap-10 px-4 py-24 md:grid-cols-2 md:px-8">
          <div class="overflow-hidden rounded-2xl">
              <img
                  src="https://images.unsplash.com/photo-1544551763-46a013bb70d5?auto=format&fit=crop&w=1400&q=80"
                  alt="Praia em Aracaju"
                  class="aspect-[4/5] w-full object-cover"
              >
          </div>
          <div class="space-y-6" data-reveal="" style="transition-delay:120ms">
              <p class="text-[10px] tracking-widest text-primary uppercase">"Nossa história"</p>
              <h2 class="font-serif text-4xl leading-tight text-foreground md:text-5xl">
                  "Moda praia feita com carinho em Aracaju"
              </h2>
              <p class="leading-relaxed text-muted-foreground">
                  "A Linda Flor nasceu para vestir mulheres que querem se sentir lindas na praia, na piscina e no pôr do sol. Qualidade no tecido, modelagem que valoriza o corpo e um atendimento próximo de quem entende moda praia de verdade."
              </p>
              <p class="text-[10px] tracking-wider text-primary uppercase">"@biquinislindaflor"</p>
          </div>
      </section>

      <section class="mx-auto max-w-7xl px-4 py-20 md:px-8">
          <div class="mb-12 text-center">
              <p class="text-[10px] tracking-widest text-primary uppercase">"Shop all"</p>
              <h2 class="font-serif mt-2 text-4xl text-foreground md:text-5xl">"Todo o catálogo"</h2>
              <p class="mx-auto mt-3 max-w-2xl text-muted-foreground">
                  "Para quem sabe que é verão o ano inteiro, encontre sua próxima peça favorita."
              </p>
          </div>
          <div class="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
              #[key(product.id.to_string())]
              for product in all_products {
                  product_card(product: product)
              }
          </div>
      </section>

      <section class="border-t border-border bg-foreground px-4 py-16 text-center text-background md:px-8">
          <p class="text-[10px] tracking-[0.24em] uppercase">"Atendimento personalizado"</p>
          <h2 class="font-serif mt-3 text-4xl md:text-5xl">"Fale com a gente no WhatsApp"</h2>
          <p class="mx-auto mt-4 max-w-xl text-sm leading-relaxed text-background/70">
              "Tire dúvidas sobre tamanhos, cores e disponibilidade. Estamos em Aracaju, de segunda a sábado, das 8h às 18h."
          </p>
          <a
              href="https://wa.me/5579998165115"
              target="_blank"
              rel="noreferrer"
              class=(class!(button_variants(ButtonVariant::Primary, ButtonSize::Lg), "mt-8"))
          >
              "Chamar no WhatsApp"
          </a>
      </section>
  })
}
