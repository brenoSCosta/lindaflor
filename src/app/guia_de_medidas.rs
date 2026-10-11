use crate::components::container::{ContainerVariant, container};
use topcoat::{
  Result,
  router::page,
  view::{View, view},
};

/// Size guide (stub; filled in by the product page task).
#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      container(
          variant: ContainerVariant::Narrow,
          <h1 class="text-4xl font-bold tracking-tight">"Guia de medidas"</h1>
      )
  })
}
