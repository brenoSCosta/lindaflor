use topcoat::{
  Result,
  context::Cx,
  router::{page, path_param},
  view::{View, view},
};

use crate::app::auth_helpers::require_admin;
use crate::components::container::container;

path_param!(pub(crate) id: String);

// TODO: Pedido details
#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  Ok(view! {
      container(
          <h1>"Admin - Pedido"</h1>
      )
  })
}
