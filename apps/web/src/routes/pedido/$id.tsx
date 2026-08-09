import { env } from "@lindaflor/env/web";
import { orderStatusLabels } from "@lindaflor/shared/enums/commerce";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { Check, Copy } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { StoreLayout } from "@/components/store/store-layout";
import { Button } from "@/components/ui/button";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/pedido/$id")({
  component: OrderPage,
});

function formatCountdown(expiresAt: Date | null | undefined) {
  if (!expiresAt) {
    return null;
  }
  const ms = expiresAt.getTime() - Date.now();
  if (ms <= 0) {
    return "Expirado";
  }
  const hours = Math.floor(ms / (1000 * 60 * 60));
  const minutes = Math.floor((ms % (1000 * 60 * 60)) / (1000 * 60));
  return `${hours}h ${minutes.toString().padStart(2, "0")}min`;
}

function OrderPage() {
  const { id } = Route.useParams();
  const [copied, setCopied] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const queryClient = useQueryClient();

  const orderQuery = useQuery({
    ...orpc.commerce.store.getOrder.queryOptions({ input: { id } }),
    refetchInterval: (query) =>
      query.state.data?.status === "pending_payment" ? 30_000 : false,
  });

  useEffect(() => {
    if (orderQuery.data?.status !== "pending_payment") {
      return undefined;
    }
    const timer = window.setInterval(() => setNow(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, [orderQuery.data?.status]);

  const order = orderQuery.data;
  const pixCode = order?.payment_meta?.pix_copy_paste;
  const pixQr = order?.payment_meta?.pix_qr_base64;
  const countdown = useMemo(() => {
    void now;
    return formatCountdown(
      order?.reservation_expires_at
        ? new Date(order.reservation_expires_at)
        : null,
    );
  }, [order?.reservation_expires_at, now]);

  async function copyPix() {
    if (!pixCode) {
      return;
    }
    await navigator.clipboard.writeText(pixCode);
    setCopied(true);
    toast.success("Código PIX copiado");
    setTimeout(() => setCopied(false), 2000);
  }

  async function simulatePayment() {
    setConfirming(true);
    try {
      const response = await fetch(`/webhooks/dev/confirm-payment/${id}`, {
        method: "POST",
      });
      if (!response.ok) {
        throw new Error("Falha ao confirmar pagamento");
      }
      await queryClient.invalidateQueries({
        queryKey: orpc.commerce.store.getOrder.key({ input: { id } }),
      });
      toast.success("Pagamento confirmado (dev)");
    } catch {
      toast.error("Não foi possível confirmar o pagamento");
    } finally {
      setConfirming(false);
    }
  }

  if (orderQuery.isLoading) {
    return (
      <StoreLayout headerVariant="solid">
        <div className="mx-auto max-w-3xl px-4 py-24 text-center text-(--lf-muted)">
          Carregando pedido…
        </div>
      </StoreLayout>
    );
  }

  if (orderQuery.isError || !order) {
    return (
      <StoreLayout headerVariant="solid">
        <div className="mx-auto max-w-3xl px-4 py-24 text-center">
          <h1 className="font-display text-4xl">Pedido não encontrado</h1>
          <Link to="/produtos" className="mt-6 inline-block">
            <Button className="rounded-none bg-(--lf-pink) uppercase">
              Voltar ao catálogo
            </Button>
          </Link>
        </div>
      </StoreLayout>
    );
  }

  return (
    <StoreLayout headerVariant="solid">
      <main className="mx-auto max-w-3xl px-4 py-12 md:px-8">
        <p className="text-[10px] tracking-[0.18em] text-(--lf-muted) uppercase">
          Pedido #{order.id.slice(0, 8)}
        </p>
        <h1 className="font-display mt-3 text-5xl text-(--lf-ink)">
          {order.status === "pending_payment"
            ? "Aguardando pagamento"
            : orderStatusLabels[order.status]}
        </h1>
        <p className="mt-2 text-(--lf-muted)">
          Enviamos as instruções para {order.guest_email}
        </p>

        {order.status === "pending_payment" ? (
          <section className="mt-10 space-y-6 border border-(--lf-line) p-6">
            <h2 className="font-display text-2xl">Pague com PIX</h2>
            <ol className="list-decimal space-y-2 pl-5 text-sm text-(--lf-muted)">
              <li>Copie o código PIX ou escaneie o QR Code</li>
              <li>
                Pague exatamente{" "}
                <strong className="text-(--lf-ink)">
                  {formatCurrency(order.total_cents)}
                </strong>
              </li>
              <li>Avise a loja no WhatsApp para confirmarmos o pagamento</li>
            </ol>

            {countdown ? (
              <p className="text-sm text-(--lf-muted)">
                Reserva de estoque expira em{" "}
                <strong className="text-(--lf-ink)">{countdown}</strong>
              </p>
            ) : null}

            {pixQr ? (
              <img
                src={`data:image/png;base64,${pixQr}`}
                alt="QR Code PIX"
                className="mx-auto size-56 border bg-white p-4"
              />
            ) : null}

            {pixCode ? (
              <div className="space-y-2">
                <p className="text-sm text-(--lf-muted)">PIX copia e cola:</p>
                <div className="flex gap-2">
                  <code className="flex-1 overflow-x-auto border bg-white p-3 text-xs">
                    {pixCode}
                  </code>
                  <Button
                    type="button"
                    variant="outline"
                    className="rounded-none"
                    onClick={() => void copyPix()}
                  >
                    {copied ? (
                      <Check className="size-4" />
                    ) : (
                      <Copy className="size-4" />
                    )}
                  </Button>
                </div>
              </div>
            ) : (
              <p className="text-sm text-amber-700">
                Instruções de pagamento indisponíveis. Fale conosco no WhatsApp.
              </p>
            )}

            {order.payment_meta?.ticket_url ? (
              <a
                href={order.payment_meta.ticket_url}
                target="_blank"
                rel="noreferrer"
                className="inline-flex w-full items-center justify-center bg-(--lf-pink) px-4 py-3 text-sm font-medium tracking-wide text-white uppercase"
              >
                Avise no WhatsApp
              </a>
            ) : null}

            {import.meta.env.DEV || env.VITE_NODE_ENV === "development" ? (
              <Button
                type="button"
                variant="outline"
                className="mt-4 w-full rounded-none"
                disabled={confirming}
                onClick={() => void simulatePayment()}
              >
                {confirming ? "Confirmando..." : "Simular pagamento (dev)"}
              </Button>
            ) : null}
          </section>
        ) : null}

        <section className="mt-10 space-y-4">
          <h2 className="text-[11px] tracking-[0.2em] uppercase">Itens</h2>
          {order.items.map((item) => (
            <div
              key={item.id}
              className="flex justify-between border-b border-(--lf-line) py-3 text-sm"
            >
              <div>
                <p className="font-medium">{item.product_name}</p>
                <p className="text-(--lf-muted)">{item.variant_label}</p>
              </div>
              <div className="text-right">
                <p>× {item.quantity}</p>
                <p>{formatCurrency(item.unit_price_cents * item.quantity)}</p>
              </div>
            </div>
          ))}
        </section>

        <section className="mt-8 space-y-2 text-sm">
          <div className="flex justify-between">
            <span className="text-(--lf-muted)">Subtotal</span>
            <span>{formatCurrency(order.subtotal_cents)}</span>
          </div>
          <div className="flex justify-between">
            <span className="text-(--lf-muted)">Frete</span>
            <span>
              {order.shipping_cents === 0
                ? "Grátis"
                : formatCurrency(order.shipping_cents)}
            </span>
          </div>
          <div className="flex justify-between pt-2 text-lg font-medium">
            <span>Total</span>
            <span className="text-(--lf-pink)">
              {formatCurrency(order.total_cents)}
            </span>
          </div>
        </section>
      </main>
    </StoreLayout>
  );
}
