pub mod id;

use topcoat::{
  Result,
  router::{href, page},
  view::{View, attributes, view},
};

use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::container::{ContainerVariant, container};

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      container(
          variant: ContainerVariant::Narrow,
          attrs: attributes! { class="py-24 text-center" },
          <h1 class="text-4xl font-bold tracking-tight">"Acompanhe seu pedido"</h1>
          <p class="mt-4 text-muted-foreground">
              "Enviamos o link do pedido para o seu e-mail. Verifique sua caixa de entrada."
          </p>
          <a
              href=(href!(crate::app::produtos::page))
              class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
          >
              "Ver catálogo"
          </a>
      )
  })
}
