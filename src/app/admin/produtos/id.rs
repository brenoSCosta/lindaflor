use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{page, path_param},
  view::{View, ViewExt, view},
};

path_param!(id: String, error = bad_request);

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
            <div style="padding: 2rem;">
                <h1>"Produto não encontrado"</h1>
                <a href="/admin/produtos">"Voltar para produtos"</a>
            </div>
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
                <p style="color: #666; margin-bottom: 0.5rem;">
                    <a href="/admin/produtos" style="color: #e94560;">"Produtos"</a>
                    " / "
                    (product.name.clone())
                </p>
                <h1 style="font-size: 1.5rem; font-weight: 600; margin-bottom: 2rem;">"Editar produto"</h1>

                <form method="post" action="/admin/produtos" style="background: white; padding: 2rem; border-radius: 8px; border: 1px solid #e5e5e5; max-width: 800px;">
                    <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem; margin-bottom: 1rem;">
                        <div>
                            <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Nome"</label>
                            <input type="text" name="name" value=(product.name) style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                        </div>
                        <div>
                            <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Slug"</label>
                            <input type="text" name="slug" value=(product.slug) style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                        </div>
                    </div>

                    <div style="margin-bottom: 1rem;">
                        <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Descrição"</label>
                        <textarea name="description" rows="3" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;">(product.description)</textarea>
                    </div>

                    <div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem; margin-bottom: 1rem;">
                        <div>
                            <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Preço (R$)"</label>
                            <input type="text" name="price" value=(product.price_in_cents / 100) style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                        </div>
                        <div>
                            <label style="display: block; font-size: 0.875rem; font-weight: 500; margin-bottom: 0.25rem;">"Categoria"</label>
                            <select name="category" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;">
                                <option value="biquini" selected="selected">"Biquíni"</option>
                                <option value="maio">"Maiô"</option>
                                <option value="saida_praia">"Saída de Praia"</option>
                                <option value="acessorio">"Acessório"</option>
                            </select>
                        </div>
                    </div>

                    <div style="display: flex; gap: 1.5rem; margin-bottom: 1.5rem;">
                        <label style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem;">
                            <input type="checkbox" name="featured" checked=(product.featured) />
                            "Destaque"
                        </label>
                        <label style="display: flex; align-items: center; gap: 0.5rem; font-size: 0.875rem;">
                            <input type="checkbox" name="active" checked=(product.active) />
                            "Ativo na loja"
                        </label>
                    </div>

                    <fieldset style="border: 1px solid #e5e5e5; border-radius: 8px; padding: 1.5rem; margin-bottom: 1.5rem;">
                        <legend style="font-weight: 500; padding: 0 0.5rem;">"Variantes"</legend>
                        <div style="display: flex; flex-direction: column; gap: 1rem;">
                            for variant in variants {
                                <div style="display: grid; grid-template-columns: repeat(4, 1fr); gap: 0.5rem; padding: 1rem; border: 1px solid #e5e5e5; border-radius: 4px;">
                                    <div>
                                        <label style="display: block; font-size: 0.75rem; color: #666; margin-bottom: 0.25rem;">"SKU"</label>
                                        <input type="text" name="variant_sku[]" value=(variant.sku) style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                                    </div>
                                    <div>
                                        <label style="display: block; font-size: 0.75rem; color: #666; margin-bottom: 0.25rem;">"Tamanho"</label>
                                        <input type="text" name="variant_size[]" value=(variant.size) style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                                    </div>
                                    <div>
                                        <label style="display: block; font-size: 0.75rem; color: #666; margin-bottom: 0.25rem;">"Cor"</label>
                                        <input type="text" name="variant_color[]" value=(variant.color) style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                                    </div>
                                    <div>
                                        <label style="display: block; font-size: 0.75rem; color: #666; margin-bottom: 0.25rem;">"Estoque baixo"</label>
                                        <input type="number" name="variant_threshold[]" value=(variant.low_stock_threshold) min="0" style="width: 100%; padding: 0.5rem; border: 1px solid #d1d5db; border-radius: 4px;" />
                                    </div>
                                </div>
                            }
                        </div>
                    </fieldset>

                    <button type="submit" style="background: #e94560; color: white; padding: 0.75rem 1.5rem; border-radius: 6px; border: none; font-weight: 500; cursor: pointer;">
                        "Salvar alterações"
                    </button>
                </form>
            </main>
        </div>
    }.boxed())
}
