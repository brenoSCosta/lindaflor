use topcoat::{
  Result,
  context::Cx,
  icon::{icon, iconify::iconify_icon},
  router::href,
  view::{View, attributes, component, view},
};

use crate::app::store::pricing::storefront_settings;
use crate::app::store::queries::whatsapp_link;

#[component]
pub async fn footer(cx: &Cx) -> Result<impl View> {
  let settings = storefront_settings(cx).await;

  let whatsapp_url = settings
    .whatsapp_number
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(|number| {
      whatsapp_link(number, "Olá! Quero saber mais sobre a Linda Flor.")
    });

  let pix_text = if settings.pix_discount_percent > 0 {
    format!(
      "Pagamento via Pix com {}% de desconto",
      settings.pix_discount_percent
    )
  } else {
    "Pagamento via Pix".to_string()
  };

  Ok(view! {
      <footer class="border-t border-border bg-background">
          <div class="mx-auto max-w-7xl px-4 py-16 md:px-8">
            // Main footer content
            <div class="grid gap-10 mb-10 sm:grid-cols-2 lg:grid-cols-4">
              <div class="space-y-3">
                <p class="font-serif text-2xl text-foreground">"Linda Flor"</p>
                <p class="text-sm leading-relaxed text-muted-foreground">
                  "Moda praia feminina com qualidade e atendimento próximo. Da praia de Aracaju para você brilhar em qualquer verão."
                </p>
                <p class="text-sm text-muted-foreground">"Rua Capitão Isaias Alves de Souza, 1100 · Aracaju, SE"</p>
              </div>

              // Institucional column
              <div class="space-y-3">
                <h3 class="text-[10px] tracking-widest uppercase font-semibold">"Institucional"</h3>
                <div class="flex flex-col gap-2 text-sm text-muted-foreground">
                  <a href=(href!(crate::app::page).fragment("sobre")) class="transition-colors hover:text-foreground">"Sobre"</a>
                  <a href=(href!(crate::app::termos::page)) class="transition-colors hover:text-foreground">"Termos"</a>
                  <a href=(href!(crate::app::politica_privacidade::page)) class="transition-colors hover:text-foreground">"Privacidade"</a>
                </div>
              </div>

              // Minha conta column
              <div class="space-y-3">
                <h3 class="text-[10px] tracking-widest uppercase font-semibold">"Minha conta"</h3>
                <div class="flex flex-col gap-2 text-sm text-muted-foreground">
                  <a href=(href!(crate::app::login::page)) class="transition-colors hover:text-foreground">"Entrar"</a>
                  <a href=(href!(crate::app::conta::pedidos::page)) class="transition-colors hover:text-foreground">"Meus pedidos"</a>
                  <a href=(href!(crate::app::conta::enderecos::page)) class="transition-colors hover:text-foreground">"Meus endereços"</a>
                  <a href=(href!(crate::app::favoritos::page)) class="transition-colors hover:text-foreground">"Favoritos"</a>
                </div>
              </div>

              // Atendimento column
              <div class="space-y-3">
                <h3 class="text-[10px] tracking-widest uppercase font-semibold">"Atendimento"</h3>
                <div class="flex flex-col gap-2 text-sm text-muted-foreground">
                  <a href=(href!(crate::app::trocas_devolucoes::page)) class="transition-colors hover:text-foreground">"Trocas e devoluções"</a>
                  <a href=(href!(crate::app::guia_de_medidas::page)) class="transition-colors hover:text-foreground">"Guia de medidas"</a>
                  if let Some(url) = whatsapp_url {
                    <a href=(url) target="_blank" rel="noreferrer" class="transition-colors hover:text-foreground">"WhatsApp"</a>
                  }
                </div>
              </div>
            </div>

            // Social and payment info
            <div class="border-t border-border pt-8 space-y-6">
              <div class="space-y-4">
                <p class="text-sm text-muted-foreground">(pix_text)</p>
                <div class="flex items-center gap-4">
                  <a href="https://www.instagram.com/biquinislindaflor/" target="_blank" rel="noreferrer" class="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors" title="Instagram">
                    icon(data: iconify_icon!("lucide:instagram"), attrs: attributes! { class="size-5" })
                    "@biquinislindaflor"
                  </a>
                </div>
              </div>
            </div>

            // Copyright
            <div class="border-t border-border mt-8 pt-8 text-center text-[10px] tracking-wider text-muted-foreground uppercase">
              "© 2026 Linda Flor Moda Praia"
            </div>
          </div>
      </footer>
  })
}
