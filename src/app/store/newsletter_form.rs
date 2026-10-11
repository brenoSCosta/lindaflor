use topcoat::{
  Result,
  router::href,
  view::{View, attributes, component, view},
};

use crate::components::button::ButtonVariant;
use crate::components::button::button;
use crate::components::input::input;
use crate::components::label::label;

/// Newsletter subscription form component.
#[component]
pub async fn newsletter_form() -> Result<impl View> {
  Ok(view! {
    <form action=(href!(crate::app::newsletter::subscribe)) method="POST" class="space-y-4">
      <div class="space-y-2">
        label(attrs: attributes! { for="email" }, "E-mail")
        input(attrs: attributes! {
          type="email"
          name="email"
          id="email"
          placeholder="seu@email.com"
          required=""
        })
      </div>

      <div class="space-y-2">
        label(attrs: attributes! { for="name" }, "Nome (opcional)")
        input(attrs: attributes! {
          type="text"
          name="name"
          id="name"
          placeholder="Seu nome"
        })
      </div>

      button(
        variant: ButtonVariant::Primary,
        attrs: attributes! { type="submit" },
        "Inscrever"
      )
    </form>
  })
}
