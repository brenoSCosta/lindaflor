import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { z } from "zod";

import { StoreLayout } from "@/components/store/store-layout";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { useCart } from "@/context/cart";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/checkout")({
  component: CheckoutPage,
  head: () => ({
    meta: [{ title: "Checkout — Linda Flor" }],
  }),
});

const storeInputClass =
  "lf-input-shadcn h-auto rounded-none border-0 border-b border-[var(--lf-line)] bg-transparent px-0 shadow-none focus-visible:border-[var(--lf-pink)] focus-visible:ring-0";

const storeLabelClass = "lf-label";

const brazilianStates = [
  "AC", "AL", "AP", "AM", "BA", "CE", "DF", "ES", "GO", "MA", "MT", "MS",
  "MG", "PA", "PB", "PR", "PE", "PI", "RJ", "RN", "RS", "RO", "RR", "SC",
  "SP", "SE", "TO",
] as const;

function CheckoutPage() {
  const navigate = useNavigate();
  const { items, subtotalCents, clearCart } = useCart();
  const [form, setForm] = useState({
    guest_email: "",
    name: "",
    street: "",
    number: "",
    complement: "",
    neighborhood: "",
    city: "",
    state: "SE",
    zip_code: "",
    notes: "",
    coupon_code: "",
  });

  const shippingQuery = useQuery({
    ...orpc.commerce.store.getShippingQuote.queryOptions({
      input: {
        state: form.state,
        zip_code: form.zip_code.replace(/\D/g, ""),
        subtotal_cents: subtotalCents,
      },
    }),
    enabled: form.zip_code.replace(/\D/g, "").length >= 8 && items.length > 0,
  });

  const createOrderMutation = useMutation(
    orpc.commerce.store.createOrder.mutationOptions({
      onSuccess: (order) => {
        clearCart();
        toast.success("Pedido criado! Pague via PIX para confirmar.");
        void navigate({ to: "/pedido/$id", params: { id: order.id } });
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const shipping = shippingQuery.data;
  const totalCents =
    subtotalCents + (shipping?.shipping_cents ?? 0);

  const canSubmit = useMemo(() => {
    const addressSchema = z.object({
      guest_email: z.email(),
      name: z.string().min(1),
      street: z.string().min(1),
      number: z.string().min(1),
      neighborhood: z.string().min(1),
      city: z.string().min(1),
      state: z.string().length(2),
      zip_code: z.string().min(8),
    });

    return (
      items.length > 0 &&
      addressSchema.safeParse({
        guest_email: form.guest_email,
        name: form.name,
        street: form.street,
        number: form.number,
        neighborhood: form.neighborhood,
        city: form.city,
        state: form.state,
        zip_code: form.zip_code.replace(/\D/g, ""),
      }).success
    );
  }, [form, items.length]);

  if (items.length === 0) {
    return (
      <StoreLayout headerVariant="solid">
        <div className="mx-auto max-w-2xl px-4 py-24 text-center md:px-8">
          <h1 className="font-display text-4xl">Seu carrinho está vazio</h1>
          <p className="mt-4 text-[var(--lf-muted)]">
            Adicione peças ao carrinho antes de finalizar a compra.
          </p>
          <Button
            className="mt-8 rounded-none bg-[var(--lf-pink)] uppercase"
            onClick={() => void navigate({ to: "/produtos" })}
          >
            Ver catálogo
          </Button>
        </div>
      </StoreLayout>
    );
  }

  function updateField<K extends keyof typeof form>(
    key: K,
    value: (typeof form)[K],
  ) {
    setForm((current) => ({ ...current, [key]: value }));
  }

  function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }

    createOrderMutation.mutate({
      guest_email: form.guest_email,
      items: items.map((item) => ({
        variant_id: item.variantId,
        quantity: item.quantity,
      })),
      shipping_address: {
        name: form.name,
        street: form.street,
        number: form.number,
        complement: form.complement || undefined,
        neighborhood: form.neighborhood,
        city: form.city,
        state: form.state,
        zip_code: form.zip_code.replace(/\D/g, ""),
      },
      notes: form.notes || undefined,
      coupon_code: form.coupon_code || undefined,
    });
  }

  return (
    <StoreLayout headerVariant="solid">
      <main className="mx-auto max-w-6xl px-4 py-12 md:px-8">
        <h1 className="lf-heading text-4xl md:text-5xl">Checkout</h1>

        <form
          onSubmit={handleSubmit}
          className="mt-10 grid gap-12 lg:grid-cols-[1.2fr_0.8fr]"
        >
          <div className="space-y-8">
            <section className="space-y-4">
              <h2 className="lf-label text-[var(--lf-ink)]">Contato</h2>
              <div className="space-y-2">
                <Label htmlFor="email" className={storeLabelClass}>
                  E-mail
                </Label>
                <Input
                  id="email"
                  type="email"
                  required
                  value={form.guest_email}
                  onChange={(event) =>
                    updateField("guest_email", event.target.value)
                  }
                  className={storeInputClass}
                />
              </div>
            </section>

            <section className="space-y-4">
              <h2 className="lf-label text-[var(--lf-ink)]">
                Endereço de entrega
              </h2>
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-2 sm:col-span-2">
                  <Label htmlFor="name">Nome completo</Label>
                  <Input
                    id="name"
                    required
                    value={form.name}
                    onChange={(event) => updateField("name", event.target.value)}
                    className={storeInputClass}
                  />
                </div>
                <div className="space-y-2 sm:col-span-2">
                  <Label htmlFor="street">Rua</Label>
                  <Input
                    id="street"
                    required
                    value={form.street}
                    onChange={(event) =>
                      updateField("street", event.target.value)
                    }
                    className={storeInputClass}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="number">Número</Label>
                  <Input
                    id="number"
                    required
                    value={form.number}
                    onChange={(event) =>
                      updateField("number", event.target.value)
                    }
                    className={storeInputClass}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="complement">Complemento</Label>
                  <Input
                    id="complement"
                    value={form.complement}
                    onChange={(event) =>
                      updateField("complement", event.target.value)
                    }
                    className={storeInputClass}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="neighborhood">Bairro</Label>
                  <Input
                    id="neighborhood"
                    required
                    value={form.neighborhood}
                    onChange={(event) =>
                      updateField("neighborhood", event.target.value)
                    }
                    className={storeInputClass}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="city">Cidade</Label>
                  <Input
                    id="city"
                    required
                    value={form.city}
                    onChange={(event) => updateField("city", event.target.value)}
                    className={storeInputClass}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="state">Estado</Label>
                  <select
                    id="state"
                    value={form.state}
                    onChange={(event) => updateField("state", event.target.value)}
                    className="h-10 w-full border-0 border-b border-[var(--lf-line)] bg-transparent px-0 text-sm outline-none focus:border-[var(--lf-pink)]"
                  >
                    {brazilianStates.map((state) => (
                      <option key={state} value={state}>
                        {state}
                      </option>
                    ))}
                  </select>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="zip">CEP</Label>
                  <Input
                    id="zip"
                    required
                    value={form.zip_code}
                    onChange={(event) =>
                      updateField("zip_code", event.target.value)
                    }
                    placeholder="49000-000"
                    className={storeInputClass}
                  />
                </div>
              </div>
            </section>

            <section className="space-y-2">
              <Label htmlFor="coupon">Cupom de desconto</Label>
              <Input
                id="coupon"
                value={form.coupon_code}
                onChange={(event) =>
                  updateField("coupon_code", event.target.value)
                }
                placeholder="Ex: LINDAFLOR10"
                className="rounded-none"
              />
            </section>

            <section className="space-y-2">
              <Label htmlFor="notes">Observações</Label>
              <Textarea
                id="notes"
                value={form.notes}
                onChange={(event) => updateField("notes", event.target.value)}
                className="rounded-none"
              />
            </section>
          </div>

          <aside className="h-fit space-y-6 bg-[var(--lf-surface-container)] p-8">
            <h2 className="lf-heading text-2xl">Resumo</h2>
            <div className="space-y-3 text-sm">
              {items.map((item) => (
                <div
                  key={item.variantId}
                  className="flex justify-between gap-4 text-[var(--lf-muted)]"
                >
                  <span>
                    {item.productName} × {item.quantity}
                  </span>
                  <span>
                    {formatCurrency(item.unitPriceCents * item.quantity)}
                  </span>
                </div>
              ))}
            </div>
            <div className="space-y-2 border-t border-[var(--lf-line)] pt-4 text-sm">
              <div className="flex justify-between">
                <span className="text-[var(--lf-muted)]">Subtotal</span>
                <span>{formatCurrency(subtotalCents)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-[var(--lf-muted)]">Frete</span>
                <span>
                  {shippingQuery.isLoading
                    ? "Calculando..."
                    : shipping?.free_shipping
                      ? "Grátis"
                      : formatCurrency(shipping?.shipping_cents ?? 0)}
                </span>
              </div>
              {shipping ? (
                <p className="text-xs text-[var(--lf-muted)]">
                  Prazo estimado: {shipping.estimated_days}
                </p>
              ) : null}
              <div className="flex justify-between pt-2 text-base font-medium">
                <span>Total</span>
                <span className="text-[var(--lf-pink)]">
                  {formatCurrency(totalCents)}
                </span>
              </div>
            </div>
            <button
              type="submit"
              disabled={!canSubmit || createOrderMutation.isPending}
              className="lf-btn-primary"
            >
              {createOrderMutation.isPending
                ? "Processando..."
                : "Pagar com PIX"}
            </button>
            <p className="text-xs text-[var(--lf-muted)]">
              Pagamento via PIX. Após confirmar, você verá o QR Code na próxima
              tela.
            </p>
          </aside>
        </form>
      </main>
    </StoreLayout>
  );
}
