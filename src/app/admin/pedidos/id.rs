use topcoat::{
  Result,
  router::page,
  view::{View, view},
};

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      <h1>"Admin - Pedido"</h1>
  })
}
