use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page},
  view::{View, attributes, view},
};

use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content};
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::textarea::textarea;

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);

  let _settings = sqlx::query!(
        "SELECT id, pix_key, pix_key_type::text as pix_key_type, whatsapp_number, whatsapp_message_template FROM store_settings LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;

  Ok(view! {
      container(
        <div class="flex flex-col gap-1.5">
          <h1 class="text-2xl font-semibold tracking-tight">"Configurações da loja"</h1>
          <p class="text-muted-foreground">"Chave PIX estática e WhatsApp usados no checkout."</p>
        </div>
          card(
              card_content(
                  <form method="post" action=(href!(crate::app::admin::configuracoes::page)) class="flex flex-col gap-8">
                      <fieldset class="space-y-4 border-0">
                          <legend class="mb-4 text-base font-semibold">"PIX (chave fixa)"</legend>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="pix_key_type" }, "Tipo da chave")
                              select(
                                  attrs: attributes! { name="pix_key_type" id="pix_key_type" },
                                  <option value="cpf">"CPF"</option>
                                  <option value="cnpj">"CNPJ"</option>
                                  <option value="email">"E-mail"</option>
                                  <option value="phone">"Telefone"</option>
                                  <option value="random">"Chave aleatória"</option>
                              )
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="pix_key" }, "Chave PIX")
                              input(attrs: attributes! {
                                  type="text"
                                  name="pix_key"
                                  id="pix_key"
                                  placeholder="CPF, CNPJ, e-mail, telefone ou chave aleatória"
                              })
                          </div>
                          <div class="grid gap-4 @sm/page:grid-cols-2">
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="pix_merchant_name" }, "Nome do recebedor")
                                  input(attrs: attributes! {
                                      type="text"
                                      name="pix_merchant_name"
                                      id="pix_merchant_name"
                                      placeholder="Linda Flor"
                                      maxlength="25"
                                  })
                                  <p class="text-xs text-muted-foreground">"Máx. 25 caracteres (Bacen)"</p>
                              </div>
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="pix_merchant_city" }, "Cidade")
                                  input(attrs: attributes! {
                                      type="text"
                                      name="pix_merchant_city"
                                      id="pix_merchant_city"
                                      placeholder="Aracaju"
                                      maxlength="15"
                                  })
                                  <p class="text-xs text-muted-foreground">"Máx. 15 caracteres (Bacen)"</p>
                              </div>
                          </div>
                      </fieldset>
                      <fieldset class="space-y-4 border-0">
                          <legend class="mb-4 text-base font-semibold">"WhatsApp da loja"</legend>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="whatsapp_number" }, "Número (com DDI)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="whatsapp_number"
                                  id="whatsapp_number"
                                  placeholder="5579998165115"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="whatsapp_message_template" }, "Modelo da mensagem do cliente")
                              textarea(attrs: attributes! {
                                  name="whatsapp_message_template"
                                  id="whatsapp_message_template"
                                  rows="3"
                              })
                              <p class="text-xs text-muted-foreground">"Use {{order_id}} e {{total}} como placeholders."</p>
                          </div>
                      </fieldset>
                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Salvar configurações"
                      )
                  </form>
              )
          )
      )
  })
}
