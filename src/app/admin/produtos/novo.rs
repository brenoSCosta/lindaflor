use topcoat::{
  Result,
  router::page,
  view::{View, view},
};

#[page]
pub async fn page() -> Result<impl View> {
  Ok(view! {
      <div style="display: flex; min-height: 100vh;">
          <aside style="width: 240px; background: #1a1a2e; color: white; padding: 1.5rem;">
              <h2 style="font-size: 1.25rem; font-weight: 600; margin-bottom: 1.5rem;">"Admin"</h2>
              <nav style="display: flex; flex-direction: column; gap: 0.5rem;">
                  <a href="/admin" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Dashboard"</a>
                  <a href="/admin/usuarios" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Usuários"</a>
                  <a href="/admin/produtos" style="color: #e94560; text-decoration: none; padding: 0.5rem; border-radius: 4px; background: rgba(233,69,96,0.1);">"Produtos"</a>
                  <a href="/admin/pedidos" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Pedidos"</a>
                  <a href="/admin/estoque" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Estoque"</a>
                  <a href="/admin/configuracoes" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Configurações"</a>
              </nav>
          </aside>
          <main style="flex: 1; padding: 2rem; background: #f5f5f5;">
              <h1 style="font-size: 1.5rem; font-weight: 600; margin-bottom: 0.5rem;">"Novo produto"</h1>
              <p style="color: #666; margin-bottom: 2rem;">"Cadastre um produto com variantes e estoque inicial."</p>

              <form method="post" action="/admin/produtos" style="background: white; padding: 2rem; border-radius: 8px; border: 1px solid #e5e5e5; max-width: 800px;">
                  <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem; margin-bottom: 1rem;">
                      <div>
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Nome"</label>
                          <input type="text" />
                      </div>
                      <div>
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Slug"</label>
                          <input type="text" name="slug" required="required" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                      </div>
                  </div>

                  <div style="margin-bottom: 1rem;">
                      <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Descrição"</label>
                      <textarea name="description" rows="3" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;"></textarea>
                  </div>

                  <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem; margin-bottom: 1rem;">
                      <div>
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Preço (R$)"</label>
                          <input type="text" name="price" placeholder="199.90" required="required" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                      </div>
                      <div>
                          <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Categoria"</label>
                          <select name="category" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;">
                              <option value="biquini">"Biquíni"</option>
                              <option value="maio">"Maiô"</option>
                              <option value="saida_praia">"Saída de Praia"</option>
                              <option value="acessorio">"Acessório"</option>
                          </select>
                      </div>
                  </div>

                  <div style="margin-bottom: 1.5rem;">
                      <label style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem;">
                          <input type="checkbox" name="featured" />
                          "Destaque na home"
                      </label>
                  </div>

                  <fieldset style="border: 1px solid #e5e5e5; border-radius: 8px; padding: 1.5rem; margin-bottom: 1.5rem;">
                      <legend style="font-weight: 500; padding: 0 0.5rem;">"Variantes"</legend>
                      <div style="display: flex; flex-direction: column; gap: 1rem;">
                          <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 0.5rem; padding: 1rem; border: 1px solid #e5e5e5; border-radius: 4px;">
                              <input type="text" name="variant_sku[]" placeholder="SKU" required="required" style="padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                              <select name="variant_size[]" style="padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;">
                                  <option value="pp">"PP"</option>
                                  <option value="p">"P"</option>
                                  <option value="m">"M"</option>
                                  <option value="g">"G"</option>
                                  <option value="gg">"GG"</option>
                              </select>
                              <input type="text" name="variant_color[]" placeholder="Cor" required="required" style="padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                              <input type="number" name="variant_quantity[]" placeholder="Estoque" min="0" value="0" required="required" style="padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                          </div>
                      </div>
                      <p style="font-size: 0.75rem; color: #666; margin-top: 0.5rem;">"Adicione mais variantes enviando o formulário e editando o produto."</p>
                  </fieldset>

                  <button type="submit" style="background: #e94560; color: white; padding: 0.75rem 1.5rem; border-radius: 6px; border: none; font-weight: 500; cursor: pointer;">
                      "Criar produto"
                  </button>
              </form>
          </main>
      </div>
  })
}
