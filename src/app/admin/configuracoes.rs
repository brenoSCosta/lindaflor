use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::page,
  view::{View, view},
};

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);

  let _settings = sqlx::query!(
        "SELECT id, pix_key, pix_key_type::text as pix_key_type, whatsapp_number, whatsapp_message_template FROM store_settings LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;

  Ok(view! {
      <div style="display: flex; min-height: 100vh;">
          <aside style="width: 240px; background: #1a1a2e; color: white; padding: 1.5rem;">
              <h2 style="font-size: 1.25rem; font-weight: 600; margin-bottom: 1.5rem;">"Admin"</h2>
              <nav style="display: flex; flex-direction: column; gap: 0.5rem;">
                  <a href="/admin" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Dashboard"</a>
                  <a href="/admin/usuarios" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Usuários"</a>
                  <a href="/admin/produtos" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Produtos"</a>
                  <a href="/admin/pedidos" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Pedidos"</a>
                  <a href="/admin/estoque" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Estoque"</a>
                  <a href="/admin/configuracoes" style="color: #e94560; text-decoration: none; padding: 0.5rem; border-radius: 4px; background: rgba(233,69,96,0.1);">"Configurações"</a>
              </nav>
          </aside>
          <main style="flex: 1; padding: 2rem; background: #f5f5f5;">
              <h1 style="font-size: 1.5rem; font-weight: 600; margin-bottom: 0.5rem;">"Configurações da loja"</h1>
              <p style="color: #666; margin-bottom: 2rem;">"Chave PIX estática e WhatsApp usados no checkout."</p>

              <form method="post" action="/admin/configuracoes" style="background: white; padding: 2rem; border-radius: 8px; border: 1px solid #e5e5e5; max-width: 600px;">
                  <fieldset style="border: none; margin-bottom: 1.5rem;">
                      <legend style="font-weight: 600; margin-bottom: 1rem;">"PIX (chave fixa)"</legend>
                      <div style="margin-bottom: 1rem;">
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Tipo da chave"</label>
                          <select name="pix_key_type" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;">
                              <option value="cpf">"CPF"</option>
                              <option value="cnpj">"CNPJ"</option>
                              <option value="email">"E-mail"</option>
                              <option value="phone">"Telefone"</option>
                              <option value="random">"Chave aleatória"</option>
                          </select>
                      </div>
                      <div style="margin-bottom: 1rem;">
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Chave PIX"</label>
                          <input type="text" name="pix_key" placeholder="CPF, CNPJ, e-mail, telefone ou chave aleatória" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                      </div>
                      <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem;">
                          <div>
                              <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Nome do recebedor"</label>
                              <input type="text" name="pix_merchant_name" placeholder="Linda Flor" maxlength="25" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                              <p style="font-size: 0.75rem; color: #666; margin-top: 0.25rem;">"Máx. 25 caracteres (Bacen)"</p>
                          </div>
                          <div>
                              <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Cidade"</label>
                              <input type="text" name="pix_merchant_city" placeholder="Aracaju" maxlength="15" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                              <p style="font-size: 0.75rem; color: #666; margin-top: 0.25rem;">"Máx. 15 caracteres (Bacen)"</p>
                          </div>
                      </div>
                  </fieldset>

                  <fieldset style="border: none; margin-bottom: 1.5rem;">
                      <legend style="font-weight: 600; margin-bottom: 1rem;">"WhatsApp da loja"</legend>
                      <div style="margin-bottom: 1rem;">
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Número (com DDI)"</label>
                          <input type="text" name="whatsapp_number" placeholder="5579998165115" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                      </div>
                      <div>
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Modelo da mensagem do cliente"</label>
                          <textarea name="whatsapp_message_template" rows="3" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;"></textarea>
                          <p style="font-size: 0.75rem; color: #666; margin-top: 0.25rem;">"Use {{order_id}} e {{total}} como placeholders."</p>
                      </div>
                  </fieldset>

                  <button type="submit" style="background: #e94560; color: white; padding: 0.75rem 1.5rem; border-radius: 6px; border: none; font-weight: 500; cursor: pointer;">
                      "Salvar configurações"
                  </button>
              </form>
          </main>
      </div>
  })
}
