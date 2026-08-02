import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { z } from "zod";

import { ProductCard } from "@/components/store/product-card";
import { StoreLayout } from "@/components/store/store-layout";
import { productCategories } from "@lindaflor/shared/enums/commerce";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";

const searchSchema = z.object({
  categoria: z.enum(productCategories).optional(),
  q: z.string().optional(),
  destaque: z.boolean().optional(),
  esgotados: z.boolean().optional(),
});

export const Route = createFileRoute("/produtos/")({
  component: ProductsPage,
  validateSearch: searchSchema,
  head: () => ({
    meta: [{ title: "Catálogo — Linda Flor" }],
  }),
});

const filters = [
  { key: "all", label: "Todos" },
  { key: "biquini", label: "Biquínis" },
  { key: "maio", label: "Maiôs" },
  { key: "saida_praia", label: "Saídas" },
  { key: "acessorio", label: "Acessórios" },
] as const;

function ProductsPage() {
  const search = Route.useSearch();

  const productsQuery = useQuery(
    orpc.commerce.store.listProducts.queryOptions({
      input: {
        category: search.categoria,
        search: search.q,
        featured: search.destaque,
        in_stock_only: !search.esgotados,
      },
    }),
  );

  const products = productsQuery.data?.data ?? [];
  const activeFilter = search.categoria ?? "all";

  return (
    <StoreLayout headerVariant="solid">
      <section className="border-b border-[var(--lf-line)] px-4 py-16 md:px-8">
        <div className="mx-auto max-w-7xl">
          <p className="text-[10px] tracking-[0.18em] text-[var(--lf-muted)] uppercase">
            Início / Catálogo
          </p>
          <h1 className="font-display mt-4 text-5xl text-[var(--lf-ink)] md:text-6xl">
            {search.q ? `Busca: ${search.q}` : "Catálogo"}
          </h1>
          <p className="mt-3 max-w-2xl text-[var(--lf-muted)]">
            Biquínis, maiôs e saídas de praia — peças selecionadas para o seu
            verão.
          </p>
        </div>
      </section>

      <section className="mx-auto max-w-7xl px-4 py-12 md:px-8">
        <div className="mb-10 flex flex-wrap gap-3">
          {filters.map((filter) => (
            <Link
              key={filter.key}
              to="/produtos"
              search={{
                categoria:
                  filter.key === "all"
                    ? undefined
                    : filter.key,
                q: search.q,
                destaque: search.destaque,
              }}
              className={cn(
                "border px-4 py-2 text-[10px] tracking-[0.16em] uppercase transition-colors",
                activeFilter === filter.key
                  ? "border-[var(--lf-pink)] bg-[var(--lf-pink)] text-white"
                  : "border-[var(--lf-line)] text-[var(--lf-muted)] hover:border-[var(--lf-pink)]",
              )}
            >
              {filter.label}
            </Link>
          ))}
        </div>

        {productsQuery.isLoading ? (
          <p className="text-[var(--lf-muted)]">Carregando catálogo...</p>
        ) : products.length === 0 ? (
          <p className="text-[var(--lf-muted)]">
            Nenhum produto encontrado.{" "}
            <Link to="/produtos" className="text-[var(--lf-pink)] underline">
              Limpar filtros
            </Link>
          </p>
        ) : (
          <>
            <p className="mb-8 text-sm text-[var(--lf-muted)]">
              {products.length} peças
            </p>
            <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-3">
              {products.map((product) => (
                <ProductCard
                  key={product.id}
                  product={product}
                  variant="editorial"
                />
              ))}
            </div>
          </>
        )}
      </section>
    </StoreLayout>
  );
}
