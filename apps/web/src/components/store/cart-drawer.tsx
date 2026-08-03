import { Link } from "@tanstack/react-router";
import { Minus, Plus, ShoppingBag, Trash2 } from "lucide-react";

import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { useCart } from "@/context/cart";
import { formatCurrency } from "@/lib/format";

type CartDrawerProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function CartDrawer({ open, onOpenChange }: CartDrawerProps) {
  const { items, itemCount, subtotalCents, updateQuantity, removeItem } =
    useCart();

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="right"
        className="store flex w-full flex-col gap-0 border-l border-(--lf-line) bg-(--lf-cream) p-0 shadow-none sm:max-w-105"
      >
        <SheetHeader className="border-b border-(--lf-line) px-6 pt-6 pb-5 pr-12">
          <SheetTitle className="lf-heading text-2xl font-normal">
            Carrinho
            {itemCount > 0 ? (
              <span className="ml-2 text-sm font-normal text-(--lf-muted)">
                ({itemCount})
              </span>
            ) : null}
          </SheetTitle>
        </SheetHeader>

        {items.length === 0 ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-5 px-6 py-8 text-center">
            <div className="flex size-16 items-center justify-center bg-(--lf-surface-container)">
              <ShoppingBag
                className="size-7 text-(--lf-muted)"
                strokeWidth={1.25}
              />
            </div>
            <div className="space-y-1">
              <p className="text-sm text-(--lf-ink)">Seu carrinho está vazio</p>
              <p className="text-xs text-(--lf-muted)">
                Explore nossa coleção de moda praia
              </p>
            </div>
            <Link to="/produtos" onClick={() => onOpenChange(false)}>
              <button type="button" className="lf-btn-outline w-auto px-8">
                Ver catálogo
              </button>
            </Link>
          </div>
        ) : (
          <>
            <div className="flex-1 divide-y divide-(--lf-line) overflow-y-auto px-6">
              {items.map((item) => (
                <div
                  key={item.variantId}
                  className="flex gap-4 py-6 first:pt-2"
                >
                  <div className="aspect-3/4 w-20 shrink-0 overflow-hidden bg-(--lf-cream-dark)">
                    {item.imageUrl ? (
                      <img
                        src={item.imageUrl}
                        alt={item.productName}
                        className="h-full w-full object-cover"
                      />
                    ) : null}
                  </div>
                  <div className="flex min-w-0 flex-1 flex-col justify-between">
                    <div>
                      <p className="text-[11px] tracking-[0.12em] text-(--lf-ink) uppercase">
                        {item.productName}
                      </p>
                      <p className="mt-1 text-xs text-(--lf-muted)">
                        {item.variantLabel}
                      </p>
                      <p className="mt-2 text-sm text-(--lf-pink)">
                        {formatCurrency(item.unitPriceCents)}
                      </p>
                    </div>
                    <div className="mt-3 flex items-center justify-between">
                      <div className="inline-flex items-center divide-x divide-(--lf-line) border border-(--lf-line)">
                        <button
                          type="button"
                          className="px-2.5 py-1.5 text-(--lf-muted) transition-colors hover:text-(--lf-ink)"
                          onClick={() =>
                            updateQuantity(item.variantId, item.quantity - 1)
                          }
                          aria-label="Diminuir quantidade"
                        >
                          <Minus className="size-3" strokeWidth={1.5} />
                        </button>
                        <span className="min-w-8 text-center text-sm">
                          {item.quantity}
                        </span>
                        <button
                          type="button"
                          className="px-2.5 py-1.5 text-(--lf-muted) transition-colors hover:text-(--lf-ink) disabled:opacity-40"
                          onClick={() =>
                            updateQuantity(item.variantId, item.quantity + 1)
                          }
                          disabled={item.quantity >= item.maxQuantity}
                          aria-label="Aumentar quantidade"
                        >
                          <Plus className="size-3" strokeWidth={1.5} />
                        </button>
                      </div>
                      <button
                        type="button"
                        onClick={() => removeItem(item.variantId)}
                        className="p-1 text-(--lf-muted) transition-colors hover:text-(--lf-pink)"
                        aria-label="Remover item"
                      >
                        <Trash2 className="size-4" strokeWidth={1.25} />
                      </button>
                    </div>
                  </div>
                </div>
              ))}
            </div>

            <div className="space-y-4 border-t border-(--lf-line) p-6">
              <div className="flex items-center justify-between text-sm">
                <span className="text-(--lf-muted)">Subtotal</span>
                <span className="font-medium text-(--lf-ink)">
                  {formatCurrency(subtotalCents)}
                </span>
              </div>
              <p className="text-xs leading-relaxed text-(--lf-muted)">
                Frete calculado no checkout. Grátis acima de R$ 299.
              </p>
              <Link to="/checkout" onClick={() => onOpenChange(false)}>
                <button type="button" className="lf-btn-primary">
                  Finalizar compra
                </button>
              </Link>
            </div>
          </>
        )}
      </SheetContent>
    </Sheet>
  );
}
