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

  let inventory = sqlx::query!(
    "SELECT i.id, i.variant_id, i.warehouse_id, i.quantity, i.reserved,
                p.name as product_name, pv.sku, pv.size::text as size, pv.color,
                w.name as warehouse_name, w.code as warehouse_code
         FROM inventory i
         JOIN product_variants pv ON pv.id = i.variant_id
         JOIN products p ON p.id = pv.product_id
         JOIN warehouses w ON w.id = i.warehouse_id
         ORDER BY p.name, pv.sku"
  )
  .fetch_all(pool)
  .await?;

  let warehouses = sqlx::query!(
    "SELECT id, code, name, is_default FROM warehouses ORDER BY name"
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
                  <a href="/admin/produtos" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Produtos"</a>
                  <a href="/admin/pedidos" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Pedidos"</a>
                  <a href="/admin/estoque" style="color: #e94560; text-decoration: none; padding: 0.5rem; border-radius: 4px; background: rgba(233,69,96,0.1);">"Estoque"</a>
                  <a href="/admin/configuracoes" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Configurações"</a>
              </nav>
          </aside>
          <main style="flex: 1; padding: 2rem; background: #f5f5f5;">
              <h1 style="font-size: 1.5rem; font-weight: 600; margin-bottom: 0.5rem;">"Estoque"</h1>
              <p style="color: #666; margin-bottom: 2rem;">"Saldo por depósito, entradas, transferências e sincronização CSV."</p>

              <div style="background: white; border-radius: 8px; border: 1px solid #e5e5e5; overflow: hidden; margin-bottom: 1.5rem;">
                  <div style="padding: 1rem 1.5rem; border-bottom: 1px solid #e5e5e5;">
                      <h3 style="font-weight: 600;">"Saldo por Depósito"</h3>
                  </div>
                  <table style="width: 100%; border-collapse: collapse;">
                      <thead>
                          <tr style="background: #f9fafb;">
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Produto"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"SKU"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Depósito"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Tamanho"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Qtd"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Reservado"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Disponível"</th>
                          </tr>
                      </thead>
                      <tbody>
                          for item in inventory {
                              <tr style="border-top: 1px solid #e5e5e5;">
                                  <td style="padding: 0.75rem 1.5rem; font-weight: 500;">(item.product_name)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(item.sku)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(item.warehouse_name)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(item.size)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(item.quantity)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(item.reserved)</td>
                                  <td style="padding: 0.75rem 1.5rem; font-weight: 500;">(item.quantity - item.reserved)</td>
                              </tr>
                          }
                      </tbody>
                  </table>
              </div>

              <div style="background: white; border-radius: 8px; border: 1px solid #e5e5e5; overflow: hidden;">
                  <div style="padding: 1rem 1.5rem; border-bottom: 1px solid #e5e5e5;">
                      <h3 style="font-weight: 600;">"Depósitos"</h3>
                  </div>
                  <table style="width: 100%; border-collapse: collapse;">
                      <thead>
                          <tr style="background: #f9fafb;">
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Código"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Nome"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Padrão"</th>
                          </tr>
                      </thead>
                      <tbody>
                          for warehouse in warehouses {
                              <tr style="border-top: 1px solid #e5e5e5;">
                                  <td style="padding: 0.75rem 1.5rem; font-family: monospace;">(warehouse.code)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(warehouse.name)</td>
                                  <td style="padding: 0.75rem 1.5rem;">
                                      if warehouse.is_default {
                                          <span style="background: #dcfce7; color: #166534; padding: 0.25rem 0.75rem; border-radius: 9999px; font-size: 0.75rem;">"Sim"</span>
                                      } else {
                                          <span>"—"</span>
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
