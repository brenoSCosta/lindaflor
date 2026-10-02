pub mod id;
pub mod novo;

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

  let products = sqlx::query!(
        "SELECT p.id::text AS \"id!\", p.name, p.slug, p.category::text AS \"category!\", p.price_in_cents, p.active,
                COUNT(DISTINCT pv.id) as variant_count,
                COALESCE(SUM(i.quantity), 0) as available_total
         FROM products p
         LEFT JOIN product_variants pv ON pv.product_id = p.id
         LEFT JOIN inventory i ON i.variant_id = pv.id
         GROUP BY p.id
         ORDER BY p.name"
    )
    .fetch_all(pool)
    .await?;

  fn category_labels(cat: &str) -> &str {
    match cat {
      "biquini" => "Biquíni",
      "maio" => "Maiô",
      "saida_praia" => "Saída de Praia",
      "acessorio" => "Acessório",
      _ => cat,
    }
  }

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
              <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem;">
                  <div>
                      <h1 style="font-size: 1.5rem; font-weight: 600; margin-bottom: 0.5rem;">"Produtos"</h1>
                      <p style="color: #666;">"Lista de produtos cadastrados no catálogo."</p>
                  </div>
                  <a href="/admin/produtos/novo" style="background: #e94560; color: white; padding: 0.75rem 1.5rem; border-radius: 6px; text-decoration: none; font-weight: 500;">
                      "Novo produto"
                  </a>
              </div>

              <div style="background: white; border-radius: 8px; border: 1px solid #e5e5e5; overflow: hidden;">
                  <table style="width: 100%; border-collapse: collapse;">
                      <thead>
                          <tr style="background: #f9fafb;">
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Nome"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Categoria"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Preço"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Variantes"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Disponível"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Status"</th>
                          </tr>
                      </thead>
                      <tbody>
                          for product in products {
                              <tr style="border-top: 1px solid #e5e5e5;">
                                  <td style="padding: 0.75rem 1.5rem; font-weight: 500;">
                                      <a href="/admin/produtos" style="color: #e94560; text-decoration: none;">
                                          (product.name)
                                      </a>
                                  </td>
                                  <td style="padding: 0.75rem 1.5rem;">(category_labels(&product.category))</td>
                                  <td style="padding: 0.75rem 1.5rem;">"R$ " (product.price_in_cents / 100)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(product.variant_count.unwrap_or(0))</td>
                                  <td style="padding: 0.75rem 1.5rem;">(product.available_total.unwrap_or(0))</td>
                                  <td style="padding: 0.75rem 1.5rem;">
                                      if product.active {
                                          <span style="background: #dcfce7; color: #166534; padding: 0.25rem 0.75rem; border-radius: 9999px; font-size: 0.75rem;">"Ativo"</span>
                                      } else {
                                          <span style="background: #fef2f2; color: #991b1b; padding: 0.25rem 0.75rem; border-radius: 9999px; font-size: 0.75rem;">"Inativo"</span>
                                      }
                                  </td>
                              </tr>
                          }
                      </tbody>
                  </table>
              </div>
          </main>
      </div>
  })
}
