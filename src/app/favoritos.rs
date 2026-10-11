use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::page,
  view::{View, view},
};

use crate::app::auth_helpers::require_user;
use crate::app::store::product_card::product_card;
use crate::app::store::wishlist;
use crate::components::container::container;

/// Page showing the user's wishlist (favoritos).
#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let user = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);

  let products = wishlist::get_wishlist(cx, pool, user.user.id).await?;
  let empty = products.is_empty();

  Ok(view! {
    container(
      <h1 class="text-4xl font-serif font-bold mb-2">"Meus Favoritos"</h1>
      <p class="text-muted-foreground mb-8">"Produtos que você curtiu"</p>

      if empty {
        <div class="text-center py-16">
          <p class="text-muted-foreground mb-4">"Você ainda não tem favoritos"</p>
          <a href="/" class="inline-block px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:opacity-90">
            "Voltar para a loja"
          </a>
        </div>
      } else {
        <div class="grid grid-cols-1 gap-6 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4">
          #[key(product.id.to_string())]
          for product in products {
            product_card(product: product)
          }
        </div>
      }
    )
  })
}
