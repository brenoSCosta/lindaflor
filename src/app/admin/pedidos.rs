pub mod id;

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

  let orders = sqlx::query!(
        "SELECT o.id::text AS id, o.guest_email, o.status::text AS status, o.total_cents, o.created_at,
                (SELECT COUNT(*) FROM order_items oi WHERE oi.order_id = o.id) as item_count
         FROM orders o
         ORDER BY o.created_at DESC
         LIMIT 50"
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
                  <a href="/admin/pedidos" style="color: #e94560; text-decoration: none; padding: 0.5rem; border-radius: 4px; background: rgba(233,69,96,0.1);">"Pedidos"</a>
                  <a href="/admin/estoque" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Estoque"</a>
                  <a href="/admin/configuracoes" style="color: #ccc; text-decoration: none; padding: 0.5rem; border-radius: 4px;">"Configurações"</a>
              </nav>
          </aside>
          <main style="flex: 1; padding: 2rem; background: #f5f5f5;">
              <h1 style="font-size: 1.5rem; font-weight: 600; margin-bottom: 0.5rem;">"Pedidos"</h1>
              <p style="color: #666; margin-bottom: 2rem;">"Acompanhe pedidos da loja e status de pagamento."</p>

              <div style="background: white; border-radius: 8px; border: 1px solid #e5e5e5; overflow: hidden;">
                  <table style="width: 100%; border-collapse: collapse;">
                      <thead>
                          <tr style="background: #f9fafb;">
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"ID"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Cliente"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Status"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Itens"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Total"</th>
                              <th style="text-align: left; padding: 0.75rem 1.5rem; font-size: 0.75rem; font-weight: 500; color: #666; text-transform: uppercase;">"Data"</th>
                          </tr>
                      </thead>
                      <tbody>
                          for order in orders {
                              <tr style="border-top: 1px solid #e5e5e5;">
                                  <td style="padding: 0.75rem 1.5rem; font-family: monospace; font-size: 0.875rem;">
                                      <a href="/admin/pedidos" style="color: #e94560; text-decoration: none;">(order.id)</a>
                                  </td>
                                  <td style="padding: 0.75rem 1.5rem;">(order.guest_email)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(order.status)</td>
                                  <td style="padding: 0.75rem 1.5rem;">(order.item_count)</td>
                                  <td style="padding: 0.75rem 1.5rem;">"R$ " (order.total_cents / 100)</td>
                                  <td style="padding: 0.75rem 1.5rem; color: #666;">(order.created_at.to_string())</td>
                              </tr>
                          }
                      </tbody>
                  </table>
              </div>
          </main>
      </div>
  })
}
