import { orderStatusLabels } from "@lindaflor/shared/enums/commerce";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";

import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/pedidos/")({
  component: AdminOrdersPage,
});

function AdminOrdersPage() {
  const ordersQuery = useQuery(
    orpc.commerce.admin.listOrders.queryOptions({ input: undefined }),
  );

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-semibold">Pedidos</h2>
        <p className="text-stone-600">
          Acompanhe pedidos da loja e status de pagamento.
        </p>
      </div>

      {ordersQuery.isLoading ? (
        <p>Carregando…</p>
      ) : ordersQuery.isError ? (
        <p className="text-red-600">
          Faça login como admin para ver os pedidos.
        </p>
      ) : ordersQuery.data?.data.length === 0 ? (
        <p className="text-stone-600">Nenhum pedido ainda.</p>
      ) : (
        <div className="overflow-hidden rounded-xl border bg-white">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>ID</TableHead>
                <TableHead>Cliente</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Itens</TableHead>
                <TableHead>Total</TableHead>
                <TableHead>Data</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {ordersQuery.data?.data.map((order) => (
                <TableRow key={order.id}>
                  <TableCell className="font-mono text-xs">
                    <Link
                      to="/admin/pedidos/$id"
                      params={{ id: order.id }}
                      className="text-pink-700 underline"
                    >
                      {order.id.slice(0, 8)}
                    </Link>
                  </TableCell>
                  <TableCell>{order.guest_email ?? "—"}</TableCell>
                  <TableCell>{orderStatusLabels[order.status]}</TableCell>
                  <TableCell>{order.item_count}</TableCell>
                  <TableCell>{formatCurrency(order.total_cents)}</TableCell>
                  <TableCell>
                    {new Date(order.created_at).toLocaleString("pt-BR")}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  );
}
