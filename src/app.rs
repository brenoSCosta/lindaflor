pub mod admin;
pub mod app_sidebar;
pub mod auth_helpers;
pub mod check_email;
pub mod checkout;
pub mod confirmar_email;
pub mod conta;
pub mod dashboard;
pub mod excluir_conta;
pub mod favoritos;
pub mod forgot_password;
pub mod guia_de_medidas;
pub mod login;
pub mod newsletter;
pub mod pedido;
pub mod politica_privacidade;
pub mod produtos;
pub mod reset_password;
pub mod sets;
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
  icon::{icon, iconify::iconify_icon},
  router::{
    BodyLimit, OriginPolicy, Router, RouterBuilderDiscoverExt, Slot,
    StatusCode, TrustedProxies,
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

use self::store::hero::hero_banners;
use self::store::pricing::storefront_settings;
use self::store::product_card::product_card;
use self::store::queries::{
  CatalogFilters, CatalogQuery, list_products_with_filters,
};
use crate::app::app_sidebar::{app_shell, is_app_shell_path, storefront_shell};
use crate::app::auth_helpers::optional_user;
use crate::auth::service;
use crate::auth::session_config;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::toast::{ToasterOptions, take_toasts, toaster};
use crate::logging::layer::RequestLoggingLayer;
use crate::rate_limit::{
  DEFAULT_BACKUP_CODE_PER_MIN, DEFAULT_GLOBAL_PER_MIN,
  DEFAULT_MP_WEBHOOK_PER_MIN, DEFAULT_PASSWORD_RESET_PER_MIN,
  DEFAULT_PEDIDO_PAGE_PER_MIN, DEFAULT_PEDIDO_POLL_PER_MIN,
  DEFAULT_SIGN_IN_PER_MIN, DEFAULT_SIGN_UP_PER_MIN, DEFAULT_TOTP_PER_MIN,
  DEFAULT_VERIFY_EMAIL_PER_MIN, RateLimitLayer, SCOPE_BACKUP_CODE,
  SCOPE_MP_WEBHOOK, SCOPE_PASSWORD_RESET, SCOPE_PEDIDO_PAGE, SCOPE_PEDIDO_POLL,
  SCOPE_SIGN_IN, SCOPE_SIGN_UP, SCOPE_TOTP, SCOPE_VERIFY_EMAIL,
};

/// Client-side reveal + `#sobre` landing behavior (see `assets/sobre.js`).
const SOBRE_SCRIPT: Asset = asset!("assets/sobre.js");

pub fn router(
  pool: PgPool,
  valkey: MultiplexedConnection,
  storage: crate::storage::ObjectStore,
  mailer: crate::mail::Mailer,
  payments: crate::payments::PaymentProvider,
) -> Router {
  let mut builder = module_router!()
    .discover()
    .cookies()
    .sessions(session_config())
    .app_context(pool)
    .app_context(valkey)
    .app_context(storage)
    .app_context(mailer)
    .app_context(payments);

  // Trust only the configured reverse proxies when resolving the
  // client IP (`valkey::client_ip` → Topcoat `client_ip`). Default 0 =
  // directly exposed: proxy headers are ignored and spoofed
  // `X-Forwarded-For` values cannot reset rate-limit or lockout buckets.
  let mut trusted_proxies =
    TrustedProxies::new().nearest(crate::config::trusted_proxy_hops());
  let proxy_networks = crate::config::trusted_proxy_networks();
  if !proxy_networks.is_empty() {
    trusted_proxies = trusted_proxies.networks(&proxy_networks);
  }
  builder = builder.trusted_proxies(trusted_proxies);

  // Same-origin only: no CorsLayer, so no Access-Control-* headers are
  // emitted and there is no public cross-origin API. Browser mutations are
  // governed by OriginPolicy (plus SameSite=Lax cookies). OriginLayer is
  // the router's outermost check: non-GET|HEAD|OPTIONS and WebSocket
  // upgrades; sec-fetch-site, else Origin==Host. No dangerous_disable.
  // Only exempt path: the Mercado Pago webhook, a cookie-less
  // server-to-server POST authenticated by its own HMAC signature.
  // Production boot requires APP_ORIGIN; when present it is also trusted
  // (and used as base_url).
  let mut origin_policy =
    OriginPolicy::new().exempt_paths([crate::payments::webhook::WEBHOOK_PATH]);
  if let Some(origin) = crate::auth::google::app_origin() {
    builder = builder.base_url(origin.as_str());
    origin_policy = origin_policy.trust_origins([origin]);
  }
  builder = builder.origin_policy(origin_policy);

  let bundle = topcoat::asset::AssetBundle::load().unwrap_or_else(|err| {
        panic!(
            "failed to load Topcoat asset bundle next to the executable: {err}\n\
             Run `topcoat asset bundle --bin lindaflor` after building (requires DATABASE_URL)."
        )
    });
  builder = builder.assets(bundle);

  // Native header fallback is outermost among application layers so 429s
  // from RateLimit still inherit queued response_headers. OriginLayer
  // remains outside this stack (Topcoat). Tower SetResponseHeader is
  // registered last (innermost) and is primary on Ok responses.
  builder = crate::security_headers::apply_security_headers_fallback(builder);

  builder = builder
    .layer(BodyLimit::max(1024 * 1024))
    .layer(BodyLimit::max(2 * 1024 * 1024).at("/settings/avatar"))
    // Product create/edit carry product fields plus up to 8 images @ 2MB each.
    .layer(BodyLimit::max(20 * 1024 * 1024).at("/admin/produtos/create"))
    .layer(BodyLimit::max(20 * 1024 * 1024).at("/admin/produtos/update"))
    .layer(RateLimitLayer::global(DEFAULT_GLOBAL_PER_MIN))
    .layer(RateLimitLayer::scoped(
      "/api/auth/sign-in",
      SCOPE_SIGN_IN,
      DEFAULT_SIGN_IN_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/sign-up",
      SCOPE_SIGN_UP,
      DEFAULT_SIGN_UP_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/request-password-reset",
      SCOPE_PASSWORD_RESET,
      DEFAULT_PASSWORD_RESET_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/reset-password",
      SCOPE_PASSWORD_RESET,
      DEFAULT_PASSWORD_RESET_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/send-verification-email",
      SCOPE_VERIFY_EMAIL,
      DEFAULT_VERIFY_EMAIL_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/verify-email",
      SCOPE_VERIFY_EMAIL,
      DEFAULT_VERIFY_EMAIL_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/two-factor/verify-totp",
      SCOPE_TOTP,
      DEFAULT_TOTP_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/api/auth/two-factor/verify-backup-code",
      SCOPE_BACKUP_CODE,
      DEFAULT_BACKUP_CODE_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      "/pedido/status-proc",
      SCOPE_PEDIDO_POLL,
      DEFAULT_PEDIDO_POLL_PER_MIN,
    ))
    .layer(RateLimitLayer::scoped(
      crate::payments::webhook::WEBHOOK_PATH,
      SCOPE_MP_WEBHOOK,
      DEFAULT_MP_WEBHOOK_PER_MIN,
    ))
    .layer(
      RateLimitLayer::scoped(
        "/pedido",
        SCOPE_PEDIDO_PAGE,
        DEFAULT_PEDIDO_PAGE_PER_MIN,
      )
      // `/pedido/status-proc` has its own tighter scope; skip it here so
      // poll requests consume exactly one budget (true override).
      .with_skips(&["/pedido/status-proc"]),
    );
  // HTML-form twins of the auth API + coupon apply (POST-only budgets).
  builder = crate::rate_limit::apply_form_rate_limits(builder);
  builder = builder.layer(RequestLoggingLayer::from_env());
  builder =
    crate::security_headers::apply_security_header_tower_layers(builder);
  builder.runtime().build()
}
#[layout]
async fn root(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
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
      <html lang="pt-BR">
          <head>
              <meta charset="UTF-8">
              <meta name="viewport" content="width=device-width, initial-scale=1.0">
              <title>"Linda Flor — Moda Praia"</title>
              <link rel="stylesheet" href=(tailwind::stylesheet!())>
              topcoat::runtime::script()
              topcoat::dev::script()
          </head>
          <body>
              if let Some(ref name) = impersonation_name {
                  <div class="flex items-center justify-center gap-4 border-b border-amber-600/40 bg-amber-100 px-4 py-2 text-sm text-amber-950">
                      <span>
                          "Você está atuando como "
                          <strong>(name.as_str())</strong>
                      </span>
                      <form method="post" action=(href!(stop_impersonating)) class="inline">
                          <button
                              type="submit"
                              class="rounded border border-amber-700/50 bg-amber-50 px-2 py-0.5 text-xs font-medium text-amber-950 hover:bg-white"
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
    .and_then(|p| crate::redirect::safe_local_redirect(&p))
    .unwrap_or_else(|| href!(crate::app::dashboard::page).resolve(cx));
  Ok(see_other(redirect))
}

// Dispatches unmatched URLs through layouts as NotFoundError (bare 404 otherwise).
not_found!("/");

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let store = crate::app::utils::object_store(cx);
  let settings = storefront_settings(cx).await;

  // The banner carousel renders nothing without active banners; the static
  // hero below fills in that case.
  let has_banners: bool = sqlx::query_scalar(
    "SELECT EXISTS(SELECT 1 FROM home_banners WHERE active = true)",
  )
  .fetch_one(pool)
  .await?;

  // Load products for different sections
  let empty_filters = CatalogFilters {
    sizes: vec![],
    colors: vec![],
    price_min_cents: None,
    price_max_cents: None,
    available_only: false,
    sale_only: false,
  };

  // Lançamentos (new arrivals, 8 products)
  let lancamentos = list_products_with_filters(
    pool,
    CatalogQuery {
      category: None,
      search: None,
      featured_only: false,
      include_out_of_stock: true,
      filters: &empty_filters,
      sort: "novos",
      limit: 8,
      offset: 0,
    },
  )
  .await?;
  let lancamentos =
    crate::app::admin::produtos::resolve_summary_images(&store, lancamentos)
      .await;

  // Mais vendidos (best sellers, 8 products)
  let mais_vendidos = list_products_with_filters(
    pool,
    CatalogQuery {
      category: None,
      search: None,
      featured_only: false,
      include_out_of_stock: true,
      filters: &empty_filters,
      sort: "mais_vendidos",
      limit: 8,
      offset: 0,
    },
  )
  .await?;
  let mais_vendidos =
    crate::app::admin::produtos::resolve_summary_images(&store, mais_vendidos)
      .await;

  // Liquida (sale only, 4 products)
  let mut sale_filters = empty_filters.clone();
  sale_filters.sale_only = true;
  let liquida = list_products_with_filters(
    pool,
    CatalogQuery {
      category: None,
      search: None,
      featured_only: false,
      include_out_of_stock: true,
      filters: &sale_filters,
      sort: "maior_desconto",
      limit: 4,
      offset: 0,
    },
  )
  .await?;
  let liquida =
    crate::app::admin::produtos::resolve_summary_images(&store, liquida).await;

  let whatsapp_url = settings
    .whatsapp_number
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(|number| {
      crate::app::store::queries::whatsapp_link(
        number,
        "Olá! Quero saber mais sobre as peças da Linda Flor.",
      )
    });

  Ok(view! {
      <script src=(SOBRE_SCRIPT) defer=""></script>

      hero_banners()
      if !has_banners {
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
      }

      <section class="mx-auto max-w-7xl px-4 py-8 md:px-8">
        <div class="grid gap-8 md:grid-cols-3">
          <div class="flex flex-col items-center text-center space-y-2">
            icon(data: iconify_icon!("lucide:truck"))
            <h3 class="font-semibold text-sm">"Frete grátis"</h3>
            <p class="text-xs text-muted-foreground">(if settings.free_shipping_threshold_cents > 0 { format!("Acima de {}", crate::app::store::queries::format_price(settings.free_shipping_threshold_cents)) } else { "Em todas as compras".to_string() })</p>
          </div>
          if settings.pix_discount_percent > 0 {
            <div class="flex flex-col items-center text-center space-y-2">
              icon(data: iconify_icon!("lucide:percent"))
              <h3 class="font-semibold text-sm">(settings.pix_discount_percent)"% OFF no Pix"</h3>
              <p class="text-xs text-muted-foreground">"Desconto imediato"</p>
            </div>
          }
          <div class="flex flex-col items-center text-center space-y-2">
            icon(data: iconify_icon!("lucide:rotate-ccw"))
            <h3 class="font-semibold text-sm">"Troca fácil"</h3>
            <p class="text-xs text-muted-foreground">"Até 7 dias"</p>
          </div>
        </div>
      </section>

      <section class="mx-auto max-w-7xl px-4 py-20 md:px-8">
        <div class="mb-10">
          <p class="text-[10px] tracking-widest text-primary uppercase">"Categorias"</p>
          <h2 class="font-serif mt-2 text-4xl text-foreground md:text-5xl">"Explore a coleção"</h2>
        </div>
        <div class="grid gap-4 md:grid-cols-3">
          <a href=(href!(crate::app::produtos::page).query([("category", "biquini")])) class="group relative aspect-[4/5] overflow-hidden rounded-lg">
            <img
              src="https://images.unsplash.com/photo-1487252502161-75020a813bf0?auto=format&fit=crop&w=1200&q=80"
              alt="Biquínis"
              class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
            >
            <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
            <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Biquínis"</p>
          </a>
          <a href=(href!(crate::app::produtos::page).query([("category", "biquini_top")])) class="group relative aspect-[4/5] overflow-hidden rounded-lg">
            <img
              src="https://images.unsplash.com/photo-1631157899275-ae088292e34f?auto=format&fit=crop&w=1200&q=80"
              alt="Tops"
              class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
            >
            <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
            <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Tops"</p>
          </a>
          <a href=(href!(crate::app::produtos::page).query([("category", "biquini_bottom")])) class="group relative aspect-[4/5] overflow-hidden rounded-lg">
            <img
              src="https://images.unsplash.com/photo-1625563854481-b1530ba713c0?auto=format&fit=crop&w=1200&q=80"
              alt="Calcinhas"
              class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
            >
            <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
            <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Calcinhas"</p>
          </a>
          <a href=(href!(crate::app::produtos::page).query([("category", "maio")])) class="group relative aspect-[4/5] overflow-hidden rounded-lg">
            <img
              src="https://images.unsplash.com/photo-1571019613454-1cb2f99b2d8b?auto=format&fit=crop&w=1200&q=80"
              alt="Maiôs"
              class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
            >
            <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
            <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Maiôs"</p>
          </a>
          <a href=(href!(crate::app::produtos::page).query([("category", "saida_praia")])) class="group relative aspect-[4/5] overflow-hidden rounded-lg">
            <img
              src="https://images.unsplash.com/photo-1519046904884-53103b34b206?auto=format&fit=crop&w=1200&q=80"
              alt="Saídas de praia"
              class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
            >
            <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
            <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Saídas"</p>
          </a>
          <a href=(href!(crate::app::produtos::page).query([("category", "acessorio")])) class="group relative aspect-[4/5] overflow-hidden rounded-lg">
            <img
              src="https://images.unsplash.com/photo-1523293182086-7651a899d37f?auto=format&fit=crop&w=1200&q=80"
              alt="Acessórios"
              class="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
            >
            <div class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent"></div>
            <p class="absolute bottom-6 left-6 font-serif text-3xl text-white">"Acessórios"</p>
          </a>
        </div>
      </section>

      if !lancamentos.is_empty() {
        <section class="border-y border-border bg-foreground/5 px-4 py-20 md:px-8">
          <div class="mx-auto max-w-7xl">
            <div class="mb-12 flex items-end justify-between gap-6">
              <div>
                <p class="text-[10px] tracking-widest text-primary uppercase">"Novidades"</p>
                <h2 class="font-serif mt-2 text-4xl text-foreground md:text-5xl">"Lançamentos"</h2>
              </div>
              <a href=(href!(crate::app::produtos::page).query([("sort", "novos")])) class="hidden items-center gap-2 text-[10px] tracking-wider uppercase md:inline-flex">
                "Ver tudo"
              </a>
            </div>
            <div class="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
              #[key(product.id.to_string())]
              for product in lancamentos {
                product_card(product: product)
              }
            </div>
          </div>
        </section>
      }

      if !mais_vendidos.is_empty() {
        <section class="mx-auto max-w-7xl px-4 py-20 md:px-8">
          <div class="mb-12 flex items-end justify-between gap-6">
            <div>
              <p class="text-[10px] tracking-widest text-primary uppercase">"Populares"</p>
              <h2 class="font-serif mt-2 text-4xl text-foreground md:text-5xl">"Mais vendidos"</h2>
            </div>
            <a href=(href!(crate::app::produtos::page).query([("sort", "mais_vendidos")])) class="hidden items-center gap-2 text-[10px] tracking-wider uppercase md:inline-flex">
              "Ver tudo"
            </a>
          </div>
          <div class="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
            #[key(product.id.to_string())]
            for product in mais_vendidos {
              product_card(product: product)
            }
          </div>
        </section>
      }

      if !liquida.is_empty() {
        <section class="bg-red-50 dark:bg-red-950/20 px-4 py-20 md:px-8">
          <div class="mx-auto max-w-7xl">
            <div class="mb-12 flex items-end justify-between gap-6">
              <div>
                <p class="text-[10px] tracking-widest text-red-600 uppercase">"Promoção"</p>
                <h2 class="font-serif mt-2 text-4xl text-foreground md:text-5xl">"Liquida"</h2>
              </div>
              <a href=(href!(crate::app::produtos::page).query([("sale", "true")])) class="hidden items-center gap-2 text-[10px] tracking-wider uppercase text-red-600 md:inline-flex font-semibold">
                "Ver mais"
              </a>
            </div>
            <div class="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
              #[key(product.id.to_string())]
              for product in liquida {
                product_card(product: product)
              }
            </div>
          </div>
        </section>
      }

      // About section
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

      <section class="border-t border-border bg-foreground px-4 py-16 text-center text-background md:px-8">
        <p class="text-[10px] tracking-[0.24em] uppercase">"Atendimento personalizado"</p>
        <h2 class="font-serif mt-3 text-4xl md:text-5xl">"Fale com a gente no WhatsApp"</h2>
        <p class="mx-auto mt-4 max-w-xl text-sm leading-relaxed text-background/70">
          "Tire dúvidas sobre tamanhos, cores e disponibilidade. Estamos em Aracaju, de segunda a sexta, das 9h às 18h."
        </p>
        if let Some(url) = whatsapp_url {
          <a
            href=(url)
            target="_blank"
            rel="noreferrer"
            class=(class!(button_variants(ButtonVariant::Primary, ButtonSize::Lg), "mt-8"))
          >
            "Chamar no WhatsApp"
          </a>
        }
      </section>
  })
}
