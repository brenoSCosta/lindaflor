import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";

import { Button } from "@/components/ui/button";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/")({
  component: AdminHomePage,
});

function AdminHomePage() {
  const alertsQuery = useQuery(
    orpc.commerce.admin.listLowStockAlerts.queryOptions({ input: undefined }),
  );

  const ordersQuery = useQuery(
    orpc.commerce.admin.listOrders.queryOptions({ input: undefined }),
  );

  const pendingOrders =
    ordersQuery.data?.data.filter((o) => o.status === "pending_payment")
      .length ?? 0;

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-semibold">Bem-vinda ao painel</h2>
        <p className="text-stone-600">
          Gerencie produtos, estoque e pedidos da Linda Flor.
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="rounded-xl border bg-white p-5">
          <p className="text-sm text-stone-500">Pedidos aguardando pagamento</p>
          <p className="mt-1 text-3xl font-semibold">{pendingOrders}</p>
          <Link to="/admin/pedidos" className="mt-3 inline-block">
            <Button size="sm" variant="outline">
              Ver pedidos
            </Button>
          </Link>
        </div>
        <div className="rounded-xl border bg-white p-5">
          <p className="text-sm text-stone-500">Alertas de estoque baixo</p>
          <p className="mt-1 text-3xl font-semibold text-amber-700">
            {alertsQuery.data?.data.length ?? 0}
          </p>
          <Link to="/admin/estoque" className="mt-3 inline-block">
            <Button size="sm" variant="outline">
              Gerenciar estoque
            </Button>
          </Link>
        </div>
      </div>

      {(alertsQuery.data?.data.length ?? 0) > 0 ? (
        <div className="rounded-xl border border-amber-200 bg-amber-50 p-5">
          <h3 className="font-medium text-amber-900">Reposição necessária</h3>
          <ul className="mt-3 space-y-2 text-sm text-amber-900">
            {alertsQuery.data?.data.slice(0, 8).map((item) => (
              <li key={`${item.variant_id}-${item.warehouse_id}`}>
                {item.product_name} · {item.sku} — {item.available} un. em{" "}
                {item.warehouse_name}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <ul className="list-disc space-y-2 pl-5 text-stone-700">
        <li>Cadastrar e editar produtos com galeria de imagens</li>
        <li>Entrada de mercadoria, transferências e ajustes por depósito</li>
        <li>Importar e exportar estoque via CSV (planilha / ERP)</li>
        <li>Acompanhar pedidos e atualizar status de envio</li>
      </ul>
    </div>
  );
}
