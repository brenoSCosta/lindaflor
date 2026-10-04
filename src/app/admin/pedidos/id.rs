use topcoat::{
  Result,
  router::{page, path_param},
  view::{View, view},
};

use crate::components::container::container;

path_param!(pub(crate) id: String);

// TODO: Pedido details
#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      container(
          <h1>"Admin - Pedido"</h1>
      )
  })
}
