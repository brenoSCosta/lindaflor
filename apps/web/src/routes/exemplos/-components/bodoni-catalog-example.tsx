import { productCategoryLabels } from "@lindaflor/shared/enums/commerce";
import { useQuery } from "@tanstack/react-query";
import { Search, ShoppingBag, User } from "lucide-react";

import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import { exampleImages } from "@/routes/exemplos/-components/example-images";

const categories = [
  { key: "all", label: "Todos" },
  { key: "biquini", label: "Biquínis" },
  { key: "maio", label: "Maiôs" },
  { key: "saida_praia", label: "Saídas" },
  { key: "featured", label: "Destaques" },
] as const;

const sizes = ["PP", "P", "M", "G", "GG"] as const;
const swatches = [
  "#1a1218",
  "#faf9f6",
  "#7c2d3a",
  "#d4147a",
  "#2d5a3d",
] as const;

export function BodoniCatalogExample() {
  const productsQuery = useQuery(
    orpc.commerce.store.listProducts.queryOptions({ input: {} }),
  );
  const products = productsQuery.data?.data ?? [];

  return (
    <div className="store-example-bodoni font-ex-body min-h-screen bg-(--ex-bg) text-(--ex-ink)">
      <header className="relative border-b border-(--ex-border)">
        <div className="mx-auto flex h-24 max-w-7xl items-center justify-between px-4 md:px-12">
          <nav className="hidden gap-8 md:flex">
            {["Coleções", "Catálogo", "Sobre", "Contato"].map((item, i) => (
              <span
                key={item}
                className={cn(
                  "text-[10px] tracking-[0.2em] uppercase",
                  i === 1 && "border-b border-(--ex-ink) pb-0.5",
                )}
              >
                {item}
              </span>
            ))}
          </nav>
          <div className="mx-auto text-center md:absolute md:left-1/2 md:-translate-x-1/2">
            <p className="font-ex-display text-3xl tracking-wide md:text-4xl">
              Linda Flor
            </p>
          </div>
          <div className="flex items-center gap-4">
            <Search className="size-4" />
            <User className="size-4" />
            <div className="relative">
              <ShoppingBag className="size-4" />
              <span className="absolute -top-1.5 -right-1.5 flex size-3.5 items-center justify-center bg-(--ex-pink) text-[8px] text-white">
                0
              </span>
            </div>
          </div>
        </div>
      </header>

      <div className="mx-auto max-w-7xl px-4 py-10 md:px-12">
        <p className="text-[10px] tracking-[0.18em] text-(--ex-muted) uppercase">
          Início / Catálogo
        </p>
        <h1 className="font-ex-display mt-4 text-5xl md:text-6xl">Catálogo</h1>
        <p className="mt-2 text-sm text-(--ex-muted)">
          Biquínis, maiôs e saídas de praia.
        </p>

        <div className="mt-12 grid gap-12 lg:grid-cols-[220px_1fr]">
          <aside className="hidden space-y-10 lg:block">
            <div>
              <p className="mb-4 text-[10px] tracking-[0.2em] uppercase">
                Categoria
              </p>
              <ul className="space-y-3">
                {categories.map((cat, i) => (
                  <li
                    key={cat.key}
                    className={cn(
                      "flex items-center gap-2 text-sm",
                      i === 0
                        ? "font-medium"
                        : "text-(--ex-muted) hover:text-(--ex-ink)",
                    )}
                  >
                    {i === 0 ? (
                      <span className="size-1.5 rounded-full bg-(--ex-pink)" />
                    ) : null}
                    {cat.label}
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <p className="mb-4 text-[10px] tracking-[0.2em] uppercase">
                Tamanho
              </p>
              <div className="flex flex-wrap gap-2">
                {sizes.map((size) => (
                  <span
                    key={size}
                    className="flex size-9 items-center justify-center border border-(--ex-border) text-[10px] tracking-wider"
                  >
                    {size}
                  </span>
                ))}
              </div>
            </div>
            <div>
              <p className="mb-4 text-[10px] tracking-[0.2em] uppercase">Cor</p>
              <div className="flex gap-2">
                {swatches.map((color) => (
                  <span
                    key={color}
                    className="size-6 rounded-full border border-(--ex-border)"
                    style={{ backgroundColor: color }}
                  />
                ))}
              </div>
            </div>
          </aside>

          <div>
            <div className="mb-8 flex flex-wrap items-center justify-between gap-4 border-b border-(--ex-border) pb-6">
              <div className="flex flex-wrap gap-4 md:hidden">
                {categories.slice(0, 4).map((cat, i) => (
                  <span
                    key={cat.key}
                    className={cn(
                      "text-[10px] tracking-[0.16em] uppercase",
                      i === 0 ? "text-(--ex-pink)" : "text-(--ex-muted)",
                    )}
                  >
                    {cat.label}
                  </span>
                ))}
              </div>
              <p className="text-xs text-(--ex-muted)">
                Exibindo 1–{products.length} de {products.length} resultados
              </p>
              <p className="text-[10px] tracking-[0.14em] uppercase">
                Ordenar por:{" "}
                <span className="text-(--ex-ink)">Mais recentes</span>
              </p>
            </div>

            {productsQuery.isLoading ? (
              <p className="text-(--ex-muted)">Carregando catálogo…</p>
            ) : (
              <div className="grid gap-x-6 gap-y-14 sm:grid-cols-2 xl:grid-cols-3">
                {products.map((product, index) => {
                  const soldOut = product.available_total === 0;
                  const image =
                    product.image_url ??
                    (index % 2 === 0
                      ? exampleImages.productLifestyle
                      : exampleImages.productDetail);

                  return (
                    <article key={product.id} className="group">
                      <div className="relative overflow-hidden bg-[#efeeeb]">
                        <div className="aspect-3/4">
                          <img
                            src={image}
                            alt={product.name}
                            className="h-full w-full object-cover"
                          />
                        </div>
                        {soldOut ? (
                          <span className="absolute top-4 left-4 bg-white/90 px-2 py-1 text-[9px] tracking-[0.16em] uppercase">
                            Esgotado
                          </span>
                        ) : null}
                        <div className="absolute inset-0 flex items-center justify-center bg-[color-mix(in_oklab,var(--ex-ink)_35%,transparent)] opacity-0 transition-opacity group-hover:opacity-100">
                          <span className="border border-white px-6 py-3 text-[10px] tracking-[0.2em] text-white uppercase">
                            Ver detalhes
                          </span>
                        </div>
                      </div>
                      <div className="mt-4 space-y-1">
                        <p className="text-[10px] tracking-[0.18em] text-(--ex-muted) uppercase">
                          {productCategoryLabels[product.category]}
                        </p>
                        <h3 className="font-ex-display text-xl">
                          {product.name}
                        </h3>
                        <p className="text-sm text-(--ex-muted)">
                          {formatCurrency(product.price_in_cents)}
                        </p>
                      </div>
                    </article>
                  );
                })}
              </div>
            )}

            <div className="mt-16 flex justify-center">
              <button
                type="button"
                className="border border-(--ex-ink) px-12 py-4 text-[10px] tracking-[0.2em] uppercase transition-colors hover:bg-(--ex-ink) hover:text-white"
              >
                Carregar mais
              </button>
            </div>
          </div>
        </div>
      </div>

      <footer className="mt-20 border-t border-(--ex-border) bg-[#efeeeb] px-4 py-16 md:px-12">
        <div className="mx-auto grid max-w-7xl gap-10 md:grid-cols-2">
          <div>
            <p className="font-ex-display text-2xl">Linda Flor</p>
            <p className="mt-3 max-w-sm text-sm leading-relaxed text-(--ex-muted)">
              Elegância e sofisticação em cada detalhe. Moda praia premium para
              mulheres exigentes.
            </p>
          </div>
          <div className="text-sm text-(--ex-muted) md:text-right">
            <p>Exemplo Bodoni · Catálogo</p>
            <p className="mt-2 text-[10px] tracking-[0.14em] uppercase">
              © 2026 Linda Flor Moda Praia
            </p>
          </div>
        </div>
      </footer>
    </div>
  );
}
