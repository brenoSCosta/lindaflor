import { productCategoryLabels } from "@lindaflor/shared/enums/commerce";
import type { ProductListItem } from "@lindaflor/shared/schemas/commerce";
import { Link } from "@tanstack/react-router";

import { formatCurrency } from "@/lib/format";
import { storeImages } from "@/lib/store-images";
import { cn } from "@/lib/utils";

type ProductCardProps = {
  product: ProductListItem;
  className?: string;
  variant?: "default" | "editorial";
  aspect?: "portrait" | "wide";
};

export function ProductCard({
  product,
  className,
  variant = "default",
  aspect = "portrait",
}: ProductCardProps) {
  const soldOut = product.available_total === 0;
  const imageSrc = product.image_url ?? storeImages.productFallback;

  if (variant === "editorial") {
    return (
      <article className={cn("group", className)}>
        <Link
          to="/produtos/$slug"
          params={{ slug: product.slug }}
          className="block"
        >
          <div className="relative overflow-hidden bg-white">
            <div
              className={cn(
                "overflow-hidden",
                aspect === "wide" ? "aspect-16/10" : "aspect-3/4",
              )}
            >
              <img
                src={imageSrc}
                alt={product.name}
                className="h-full w-full object-cover transition-transform duration-700 group-hover:scale-[1.03]"
              />
            </div>
            {soldOut ? (
              <span className="absolute top-4 left-4 bg-white/90 px-2 py-1 text-[9px] tracking-[0.16em] text-(--lf-ink) uppercase">
                Esgotado
              </span>
            ) : null}
          </div>
          <div className="space-y-1 pt-4">
            <p className="text-[10px] tracking-[0.18em] text-muted uppercase">
              {product.name}
            </p>
            <p className="font-display text-xl text-(--lf-ink)">
              {formatCurrency(product.price_in_cents)}
            </p>
          </div>
        </Link>
      </article>
    );
  }

  return (
    <article className={cn("group", className)}>
      <Link
        to="/produtos/$slug"
        params={{ slug: product.slug }}
        className="block"
      >
        <div className="relative overflow-hidden bg-(--lf-cream-dark)">
          <div className="aspect-3/4 overflow-hidden">
            <img
              src={imageSrc}
              alt={product.name}
              className={cn(
                "h-full w-full object-cover transition-transform duration-700 ease-out group-hover:scale-[1.04]",
                soldOut && "opacity-60 grayscale",
              )}
            />
          </div>

          {product.featured ? (
            <span className="absolute top-4 left-4 bg-(--lf-pink) px-3 py-1 text-[10px] font-semibold tracking-[0.18em] text-(--lf-cream) uppercase">
              Destaque
            </span>
          ) : null}

          {soldOut ? (
            <span className="absolute top-4 right-4 bg-(--lf-ink) px-3 py-1 text-[10px] font-semibold tracking-[0.18em] text-(--lf-cream) uppercase">
              Esgotado
            </span>
          ) : null}

          <div className="absolute inset-x-0 bottom-0 translate-y-full bg-[color-mix(in_oklab,var(--lf-ink)_82%,transparent)] px-4 py-3 text-center text-[11px] tracking-[0.18em] text-(--lf-cream) uppercase backdrop-blur-sm transition-transform duration-300 group-hover:translate-y-0">
            Ver detalhes
          </div>
        </div>

        <div className="space-y-2 pt-4">
          <p className="text-[10px] tracking-[0.2em] text-muted uppercase">
            {productCategoryLabels[product.category]}
          </p>
          <h3 className="font-display text-2xl leading-tight text-(--lf-ink)">
            {product.name}
          </h3>
          <p className="text-sm font-medium text-(--lf-pink)">
            {formatCurrency(product.price_in_cents)}
          </p>
        </div>
      </Link>
    </article>
  );
}
