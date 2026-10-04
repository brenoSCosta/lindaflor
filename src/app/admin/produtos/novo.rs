use topcoat::{
  Result,
  router::page,
  view::{View, attributes, view},
};

use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::textarea::textarea;

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      container(
          <h1 class="text-2xl font-semibold tracking-tight">"Novo produto"</h1>
          <p class="text-muted-foreground">"Cadastre um produto com variantes e estoque inicial."</p>

          card(
              card_content(
                  <form method="post" action="/admin/produtos" class="flex flex-col gap-4">
                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="name" }, "Nome")
                              input(attrs: attributes! { type="text" id="name" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="slug" }, "Slug")
                              input(attrs: attributes! {
                                  type="text"
                                  name="slug"
                                  id="slug"
                                  required=""
                              })
                          </div>
                      </div>

                      <div class="space-y-2">
                          label(attrs: attributes! { for="description" }, "Descrição")
                          textarea(attrs: attributes! { name="description" id="description" rows="3" })
                      </div>

                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="price" }, "Preço (R$)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="price"
                                  id="price"
                                  placeholder="199.90"
                                  required=""
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="category" }, "Categoria")
                              select(
                                  attrs: attributes! { name="category" id="category" },
                                  <option value="biquini">"Biquíni"</option>
                                  <option value="maio">"Maiô"</option>
                                  <option value="saida_praia">"Saída de Praia"</option>
                                  <option value="acessorio">"Acessório"</option>
                              )
                          </div>
                      </div>

                      <div class="flex items-center gap-2">
                          checkbox(attrs: attributes! { id="featured" name="featured" })
                          label(attrs: attributes! { for="featured" }, "Destaque na home")
                      </div>

                      <fieldset class="space-y-4 rounded-xl border border-border p-2">
                          <legend class="px-2 font-medium">"Variantes"</legend>
                          <div class="flex flex-col gap-4">
                              <div class="grid grid-cols-1 gap-4 rounded-lg border border-border p-4 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                                  <div class="space-y-2">
                                      label("SKU")
                                      input(attrs: attributes! {
                                          type="text"
                                          name="variant_sku[]"
                                          placeholder="SKU"
                                          required=""
                                      })
                                  </div>
                                  <div class="space-y-2">
                                      label("Tamanho")
                                      select(
                                          attrs: attributes! { name="variant_size[]" },
                                          <option value="pp">"PP"</option>
                                          <option value="p">"P"</option>
                                          <option value="m">"M"</option>
                                          <option value="g">"G"</option>
                                          <option value="gg">"GG"</option>
                                      )
                                  </div>
                                  <div class="space-y-2">
                                      label("Cor")
                                      input(attrs: attributes! {
                                          type="text"
                                          name="variant_color[]"
                                          placeholder="Cor"
                                          required=""
                                      })
                                  </div>
                                  <div class="space-y-2">
                                      label("Estoque")
                                      input(attrs: attributes! {
                                          type="number"
                                          name="variant_quantity[]"
                                          placeholder="Estoque"
                                          min="0"
                                          value="0"
                                          required=""
                                      })
                                  </div>
                              </div>
                          </div>
                          <p class="text-xs text-muted-foreground">"Adicione mais variantes enviando o formulário e editando o produto."</p>
                      </fieldset>

                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Criar produto"
                      )
                  </form>
              )
          )
      )
  })
}
