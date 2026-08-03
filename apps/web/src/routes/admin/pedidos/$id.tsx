import {
  orderStatusLabels,
  type orderStatuses,
} from "@lindaflor/shared/enums/commerce";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/admin/pedidos/$id")({
  component: AdminOrderDetailPage,
});

const nextStatuses: Record<
  (typeof orderStatuses)[number],
  (typeof orderStatuses)[number][]
> = {
  pending_payment: ["paid", "cancelled"],
  paid: ["processing", "cancelled"],
  processing: ["shipped", "cancelled"],
  shipped: ["delivered"],
  delivered: [],
  cancelled: [],
};

function AdminOrderDetailPage() {
  const { id } = Route.useParams();
  const queryClient = useQueryClient();

  const orderQuery = useQuery(
    orpc.commerce.admin.getOrder.queryOptions({ input: { id } }),
  );

  const updateMutation = useMutation(
    orpc.commerce.admin.updateOrderStatus.mutationOptions({
      onSuccess: async () => {
        await queryClient.invalidateQueries({
          queryKey: orpc.commerce.admin.getOrder.key({ input: { id } }),
        });
        toast.success("Status atualizado");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  if (orderQuery.isLoading) {
    return <p>Carregando pedido…</p>;
  }

  if (orderQuery.isError || !orderQuery.data) {
    return <p className="text-red-600">Pedido não encontrado.</p>;
  }

  const order = orderQuery.data;
  const actions = nextStatuses[order.status];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="text-sm text-stone-500">
            <Link to="/admin/pedidos">Pedidos</Link> / #{order.id.slice(0, 8)}
          </p>
          <h2 className="text-2xl font-semibold">
            {orderStatusLabels[order.status]}
          </h2>
          <p className="text-stone-600">{order.guest_email}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          {actions.map((status) => (
            <Button
              key={status}
              variant={status === "cancelled" ? "outline" : "default"}
              size="sm"
              disabled={updateMutation.isPending}
              onClick={() => updateMutation.mutate({ id: order.id, status })}
            >
              {orderStatusLabels[status]}
            </Button>
          ))}
        </div>
      </div>

      <div className="rounded-xl border bg-white p-6">
        <h3 className="mb-4 font-medium">Itens</h3>
        <div className="space-y-3">
          {order.items.map((item) => (
            <div
              key={item.id}
              className="flex justify-between border-b pb-3 text-sm"
            >
              <div>
                <p className="font-medium">{item.product_name}</p>
                <p className="text-stone-500">{item.variant_label}</p>
              </div>
              <div className="text-right">
                <p>× {item.quantity}</p>
                <p>{formatCurrency(item.unit_price_cents * item.quantity)}</p>
              </div>
            </div>
          ))}
        </div>
        <div className="mt-4 space-y-1 text-sm">
          <div className="flex justify-between">
            <span>Subtotal</span>
            <span>{formatCurrency(order.subtotal_cents)}</span>
          </div>
          <div className="flex justify-between">
            <span>Frete</span>
            <span>{formatCurrency(order.shipping_cents)}</span>
          </div>
          {order.discount_cents > 0 ? (
            <div className="flex justify-between text-green-700">
              <span>Desconto</span>
              <span>-{formatCurrency(order.discount_cents)}</span>
            </div>
          ) : null}
          <div className="flex justify-between pt-2 text-base font-semibold">
            <span>Total</span>
            <span>{formatCurrency(order.total_cents)}</span>
          </div>
        </div>
      </div>

      {order.shipping_address ? (
        <div className="rounded-xl border bg-white p-6 text-sm">
          <h3 className="mb-2 font-medium">Endereço de entrega</h3>
          <p>{order.shipping_address.name}</p>
          <p>
            {order.shipping_address.street}, {order.shipping_address.number}
          </p>
          <p>
            {order.shipping_address.neighborhood} —{" "}
            {order.shipping_address.city}/{order.shipping_address.state}
          </p>
          <p>CEP {order.shipping_address.zip_code}</p>
        </div>
      ) : null}
    </div>
  );
}
