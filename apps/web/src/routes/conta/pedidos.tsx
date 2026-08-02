import { createFileRoute, Link, redirect } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import { StoreLayout } from "@/components/store/store-layout";
import { Button } from "@/components/ui/button";
import { orderStatusLabels } from "@lindaflor/shared/enums/commerce";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/conta/pedidos")({
  beforeLoad: async ({ context }) => {
    const result = await context.auth.getSession();
    if (!result.data) {
      throw redirect({ to: "/login" });
    }
  },
  component: MyOrdersPage,
});

function MyOrdersPage() {
  const ordersQuery = useQuery(
    orpc.commerce.store.listMyOrders.queryOptions({ input: undefined }),
  );

  return (
    <StoreLayout headerVariant="solid">
      <main className="mx-auto max-w-3xl px-4 py-12 md:px-8">
        <h1 className="font-display text-4xl">Meus pedidos</h1>
        {ordersQuery.isLoading ? (
          <p className="mt-6 text-[var(--lf-muted)]">Carregando...</p>
        ) : ordersQuery.data?.data.length === 0 ? (
          <div className="mt-8 space-y-4">
            <p className="text-[var(--lf-muted)]">Você ainda não fez pedidos.</p>
            <Link to="/produtos">
              <Button className="rounded-none bg-[var(--lf-pink)] uppercase">
                Ver catálogo
              </Button>
            </Link>
          </div>
        ) : (
          <div className="mt-8 space-y-4">
            {ordersQuery.data?.data.map((order) => (
              <Link
                key={order.id}
                to="/pedido/$id"
                params={{ id: order.id }}
                className="block border border-[var(--lf-line)] p-4 transition-colors hover:border-[var(--lf-pink)]"
              >
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <p className="font-medium">#{order.id.slice(0, 8)}</p>
                    <p className="text-sm text-[var(--lf-muted)]">
                      {orderStatusLabels[order.status]} · {order.item_count} itens
                    </p>
                  </div>
                  <p className="text-[var(--lf-pink)]">
                    {formatCurrency(order.total_cents)}
                  </p>
                </div>
              </Link>
            ))}
          </div>
        )}
      </main>
    </StoreLayout>
  );
}
