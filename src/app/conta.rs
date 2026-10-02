pub mod pedidos;

use topcoat::{
  Result,
  context::Cx,
  router::{error::RouterErrorExt, page},
  view::{View, view},
};

use lindaflor::auth::user::current_user_owned;

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _su = current_user_owned(cx).await?.ok_or_redirect("/login")?;

  Ok(view! {
      <div class="mx-auto max-w-7xl px-4 py-8 md:px-8">
          <h1 class="text-2xl font-bold">"Minha Conta"</h1>
          <p class="mt-2 text-sm text-muted-foreground">
              "Acesse seus "
              <a href="/conta/pedidos" class="text-primary">"pedidos"</a>
              ", "
              <a href="/settings" class="text-primary">"configurações"</a>
              " ou o "
              <a href="/dashboard" class="text-primary">"painel"</a>
              "."
          </p>
      </div>
  })
}
