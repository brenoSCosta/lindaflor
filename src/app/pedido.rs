pub mod id;

use topcoat::{
  Result,
  router::page,
  view::{View, view},
};

use crate::components::button::{ButtonSize, ButtonVariant, button_variants};

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      <main class="mx-auto max-w-3xl px-4 py-24 text-center md:px-8">
          <h1 class="text-4xl font-bold tracking-tight">"Acompanhe seu pedido"</h1>
          <p class="mt-4 text-muted-foreground">
              "Enviamos o link do pedido para o seu e-mail. Verifique sua caixa de entrada."
          </p>
          <a
              href="/produtos"
              class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
          >
              "Ver catálogo"
          </a>
      </main>
  })
}
