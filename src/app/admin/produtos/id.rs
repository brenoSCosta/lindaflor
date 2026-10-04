use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page, path_param},
  view::{View, ViewExt, attributes, view},
};

use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::textarea::textarea;

path_param!(pub(crate) id: String, error = bad_request);

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let id = path_param::<Id>(cx)?;

  let product = sqlx::query!(
        "SELECT id::text AS \"id!\", name, slug, description, price_in_cents, category::text AS \"category!\", featured, active FROM products WHERE id::text = $1",
        id.to_string()
    )
    .fetch_optional(pool)
    .await?;

  let product = match product {
    Some(p) => p,
    None => {
      return Ok(
        view! {
            container(
                <h1 class="text-2xl font-semibold tracking-tight">"Produto não encontrado"</h1>
                <a href=(href!(crate::app::admin::produtos::page)) class="text-primary hover:underline">
                    "Voltar para produtos"
                </a>
            )
        }
        .boxed(),
      );
    }
  };

  let variants = sqlx::query!(
        "SELECT id::text AS \"id!\", sku, size::text AS \"size!\", color, price_in_cents, low_stock_threshold FROM product_variants WHERE product_id::text = $1 ORDER BY created_at",
        id.to_string()
    )
    .fetch_all(pool)
    .await?;

  Ok(view! {
      container(
          <p class="text-muted-foreground">
              <a href=(href!(crate::app::admin::produtos::page)) class="text-primary hover:underline">"Produtos"</a>
              " / "
              (product.name.clone())
          </p>
          <h1 class="text-2xl font-semibold tracking-tight">"Editar produto"</h1>

          card(
              card_content(
                  <form method="post" action="/admin/produtos" class="flex flex-col gap-4">
                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="name" }, "Nome")
                              input(attrs: attributes! {
                                  type="text"
                                  name="name"
                                  id="name"
                                  value=(product.name)
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="slug" }, "Slug")
                              input(attrs: attributes! {
                                  type="text"
                                  name="slug"
                                  id="slug"
                                  value=(product.slug)
                              })
                          </div>
                      </div>

                      <div class="space-y-2">
                          label(attrs: attributes! { for="description" }, "Descrição")
                          textarea(
                              attrs: attributes! { name="description" id="description" rows="3" },
                              (product.description)
                          )
                      </div>

                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="price" }, "Preço (R$)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="price"
                                  id="price"
                                  value=(product.price_in_cents / 100)
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="category" }, "Categoria")
                              select(
                                  attrs: attributes! { name="category" id="category" },
                                  <option value="biquini" selected="selected">"Biquíni"</option>
                                  <option value="maio">"Maiô"</option>
                                  <option value="saida_praia">"Saída de Praia"</option>
                                  <option value="acessorio">"Acessório"</option>
                              )
                          </div>
                      </div>

                      <div class="flex flex-wrap items-center gap-6">
                          <div class="flex items-center gap-2">
                              checkbox(attrs: attributes! {
                                  id="featured"
                                  name="featured"
                                  checked=(product.featured)
                              })
                              label(attrs: attributes! { for="featured" }, "Destaque")
                          </div>
                          <div class="flex items-center gap-2">
                              checkbox(attrs: attributes! {
                                  id="active"
                                  name="active"
                                  checked=(product.active)
                              })
                              label(attrs: attributes! { for="active" }, "Ativo na loja")
                          </div>
                      </div>

                      <fieldset class="space-y-4 rounded-xl border border-border p-2">
                          <legend class="px-2 font-medium">"Variantes"</legend>
                          <div class="flex flex-col gap-4">
                              for variant in variants {
                                  <div class="grid grid-cols-1 gap-4 rounded-lg border border-border p-4 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                                      <div class="space-y-2">
                                          label("SKU")
                                          input(attrs: attributes! {
                                              type="text"
                                              name="variant_sku[]"
                                              value=(variant.sku)
                                          })
                                      </div>
                                      <div class="space-y-2">
                                          label("Tamanho")
                                          input(attrs: attributes! {
                                              type="text"
                                              name="variant_size[]"
                                              value=(variant.size)
                                          })
                                      </div>
                                      <div class="space-y-2">
                                          label("Cor")
                                          input(attrs: attributes! {
                                              type="text"
                                              name="variant_color[]"
                                              value=(variant.color)
                                          })
                                      </div>
                                      <div class="space-y-2">
                                          label("Estoque baixo")
                                          input(attrs: attributes! {
                                              type="number"
                                              name="variant_threshold[]"
                                              value=(variant.low_stock_threshold)
                                              min="0"
                                          })
                                      </div>
                                  </div>
                              }
                          </div>
                      </fieldset>

                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Salvar alterações"
                      )
                  </form>
              )
          )
      )
  }.boxed())
}
