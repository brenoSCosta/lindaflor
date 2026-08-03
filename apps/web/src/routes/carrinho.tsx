import { createFileRoute, Link } from "@tanstack/react-router";
import { Minus, Plus, Trash2 } from "lucide-react";

import { StoreLayout } from "@/components/store/store-layout";
import { useCart } from "@/context/cart";
import { formatCurrency } from "@/lib/format";

export const Route = createFileRoute("/carrinho")({
  component: CartPage,
});

function CartPage() {
  const { items, subtotalCents, itemCount, updateQuantity, removeItem } =
    useCart();

  return (
    <StoreLayout headerVariant="solid">
      <main className="mx-auto max-w-3xl px-4 py-12 md:px-8 md:py-16">
        <header className="mb-10">
          <h1 className="lf-heading text-4xl md:text-5xl">Carrinho</h1>
          {itemCount > 0 ? (
            <p className="mt-2 text-sm text-muted">
              {itemCount} {itemCount === 1 ? "peça" : "peças"}
            </p>
          ) : null}
        </header>

        {items.length === 0 ? (
          <div className="py-16 text-center">
            <p className="text-muted">Seu carrinho está vazio.</p>
            <Link to="/produtos" className="mt-6 inline-block">
              <button type="button" className="lf-btn-primary w-auto px-10">
                Ver catálogo
              </button>
            </Link>
          </div>
        ) : (
          <div className="space-y-0">
            <div className="divide-y divide-[var(--lf-line)]">
              {items.map((item) => (
                <div
                  key={item.variantId}
                  className="grid grid-cols-[5rem_1fr_auto] items-center gap-4 py-6 sm:grid-cols-[7rem_1fr_auto]"
                >
                  <div className="aspect-[3/4] overflow-hidden bg-[var(--lf-cream-dark)]">
                    {item.imageUrl ? (
                      <img
                        src={item.imageUrl}
                        alt={item.productName}
                        className="h-full w-full object-cover"
                      />
                    ) : null}
                  </div>

                  <div className="min-w-0 space-y-1">
                    <p className="text-[11px] tracking-[0.12em] text-[var(--lf-ink)] uppercase">
                      {item.productName}
                    </p>
                    <p className="text-xs text-muted">{item.variantLabel}</p>
                    <div className="flex items-center gap-3 pt-2">
                      <div className="inline-flex items-center divide-x divide-[var(--lf-line)] border border-[var(--lf-line)]">
                        <button
                          type="button"
                          className="px-2.5 py-1.5 text-muted hover:text-[var(--lf-ink)]"
                          onClick={() =>
                            updateQuantity(item.variantId, item.quantity - 1)
                          }
                          aria-label="Diminuir"
                        >
                          <Minus className="size-3" strokeWidth={1.5} />
                        </button>
                        <span className="min-w-8 text-center text-sm">
                          {item.quantity}
                        </span>
                        <button
                          type="button"
                          className="px-2.5 py-1.5 text-muted hover:text-[var(--lf-ink)] disabled:opacity-40"
                          onClick={() =>
                            updateQuantity(item.variantId, item.quantity + 1)
                          }
                          disabled={item.quantity >= item.maxQuantity}
                          aria-label="Aumentar"
                        >
                          <Plus className="size-3" strokeWidth={1.5} />
                        </button>
                      </div>
                      <button
                        type="button"
                        onClick={() => removeItem(item.variantId)}
                        className="text-muted hover:text-[var(--lf-pink)]"
                        aria-label="Remover"
                      >
                        <Trash2 className="size-4" strokeWidth={1.25} />
                      </button>
                    </div>
                  </div>

                  <p className="text-right text-sm text-[var(--lf-pink)]">
                    {formatCurrency(item.unitPriceCents * item.quantity)}
                  </p>
                </div>
              ))}
            </div>

            <div className="mt-10 space-y-6 border-t border-[var(--lf-line)] pt-8">
              <div className="flex items-center justify-between">
                <span className="text-sm text-muted">Subtotal</span>
                <span className="text-xl text-[var(--lf-pink)]">
                  {formatCurrency(subtotalCents)}
                </span>
              </div>
              <p className="text-xs text-muted">
                Frete grátis para compras acima de R$ 299. Calculado no
                checkout.
              </p>
              <div className="flex flex-col gap-3 sm:flex-row">
                <Link to="/produtos" className="sm:flex-1">
                  <button type="button" className="lf-btn-outline">
                    Continuar comprando
                  </button>
                </Link>
                <Link to="/checkout" className="sm:flex-1">
                  <button type="button" className="lf-btn-primary">
                    Finalizar compra
                  </button>
                </Link>
              </div>
            </div>
          </div>
        )}
      </main>
    </StoreLayout>
  );
}
