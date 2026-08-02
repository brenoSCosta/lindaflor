import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight, Search, ShoppingBag, User } from "lucide-react";

import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";
import { exampleImages } from "@/routes/exemplos/-components/example-images";
import { cn } from "@/lib/utils";

const nav = ["Início", "Catálogo", "Coleções", "Sobre"] as const;

const categories = [
  { label: "Biquínis", image: exampleImages.categoryBiquini },
  { label: "Maiôs", image: exampleImages.categoryMaio },
  { label: "Saídas de praia", image: exampleImages.categorySaida },
] as const;

export function EditorialHomeExample() {
  const featuredQuery = useQuery(
    orpc.commerce.store.listProducts.queryOptions({
      input: { featured: true },
    }),
  );
  const products = featuredQuery.data?.data ?? [];

  return (
    <div className="store-example-editorial font-ex-body min-h-screen bg-[var(--ex-bg)] text-[var(--ex-ink)]">
      <div className="border-b border-[color-mix(in_oklab,var(--ex-ink)_8%,transparent)] bg-[var(--ex-bg)] px-4 py-2 text-center text-[10px] tracking-[0.2em] text-[var(--ex-muted)] uppercase">
        Frete grátis acima de R$299 · Aracaju, SE
      </div>

      <header className="sticky top-0 z-40 border-b border-[color-mix(in_oklab,var(--ex-ink)_6%,transparent)] bg-[color-mix(in_oklab,var(--ex-bg)_94%,white)] backdrop-blur-md">
        <div className="mx-auto grid h-20 max-w-7xl grid-cols-[1fr_auto_1fr] items-center px-4 md:px-8">
          <nav className="hidden gap-6 md:flex">
            {nav.map((item) => (
              <span
                key={item}
                className="cursor-default text-[10px] tracking-[0.18em] uppercase"
              >
                {item}
              </span>
            ))}
          </nav>
          <Link to="/exemplos/editorial" className="text-center">
            <span className="font-ex-display text-2xl italic md:text-3xl">
              Linda Flor
            </span>
            <span className="mt-0.5 block text-[9px] tracking-[0.28em] text-[var(--ex-muted)] uppercase">
              Moda Praia
            </span>
          </Link>
          <div className="flex items-center justify-end gap-2">
            <button type="button" aria-label="Buscar">
              <Search className="size-4" />
            </button>
            <button type="button" aria-label="Conta">
              <User className="size-4" />
            </button>
            <button type="button" aria-label="Carrinho">
              <ShoppingBag className="size-4" />
            </button>
          </div>
        </div>
      </header>

      <section className="relative min-h-[85vh] overflow-hidden">
        <img
          src={exampleImages.heroBeach}
          alt=""
          className="absolute inset-0 h-full w-full object-cover"
        />
        <div className="absolute inset-0 bg-gradient-to-t from-[color-mix(in_oklab,var(--ex-ink)_55%,transparent)] via-[color-mix(in_oklab,var(--ex-ink)_10%,transparent)] to-transparent" />
        <div className="relative mx-auto flex min-h-[85vh] max-w-7xl items-end px-4 pb-16 md:px-8 md:pb-24">
          <div className="max-w-xl space-y-6 text-white">
            <p className="text-[10px] tracking-[0.24em] text-white/75 uppercase">
              Coleção Verão 2026
            </p>
            <h1 className="font-ex-display text-5xl leading-[0.95] md:text-7xl lg:text-8xl">
              Elegância tropical
            </h1>
            <p className="max-w-md text-sm leading-relaxed text-white/80 md:text-base">
              Peças pensadas para quem ama sol, mar e estilo. Fotografia em
              primeiro plano — sem blocos de logo, só moda praia de verdade.
            </p>
            <div className="flex flex-wrap gap-3 pt-2">
              <button
                type="button"
                className="inline-flex h-12 items-center bg-[var(--ex-pink)] px-8 text-[10px] tracking-[0.2em] uppercase"
              >
                Comprar agora
                <ArrowRight className="ml-2 size-4" />
              </button>
              <button
                type="button"
                className="inline-flex h-12 items-center border border-white/50 px-8 text-[10px] tracking-[0.2em] uppercase backdrop-blur-sm"
              >
                Ver coleções
              </button>
            </div>
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-7xl px-4 py-20 md:px-8">
        <div className="mb-10 flex items-end justify-between">
          <div>
            <p className="text-[10px] tracking-[0.22em] text-[var(--ex-pink)] uppercase">
              Shop by category
            </p>
            <h2 className="font-ex-display mt-2 text-4xl md:text-5xl">
              Explore a coleção
            </h2>
          </div>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          {categories.map((category) => (
            <div
              key={category.label}
              className="group relative aspect-[4/5] overflow-hidden"
            >
              <img
                src={category.image}
                alt={category.label}
                className="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
              />
              <div className="absolute inset-0 bg-gradient-to-t from-[color-mix(in_oklab,var(--ex-ink)_50%,transparent)] to-transparent" />
              <p className="absolute bottom-6 left-6 font-ex-display text-3xl text-white">
                {category.label}
              </p>
            </div>
          ))}
        </div>
      </section>

      <section className="border-y border-[color-mix(in_oklab,var(--ex-ink)_6%,transparent)] bg-[var(--ex-cream-dark)] px-4 py-20 md:px-8">
        <div className="mx-auto max-w-7xl">
          <p className="text-[10px] tracking-[0.22em] text-[var(--ex-pink)] uppercase">
            Destaques
          </p>
          <h2 className="font-ex-display mt-2 mb-12 text-4xl md:text-5xl">
            Os favoritos da temporada
          </h2>
          {featuredQuery.isLoading ? (
            <p className="text-[var(--ex-muted)]">Carregando...</p>
          ) : (
            <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
              {products.map((product, index) => (
                <article key={product.id} className={cn(index === 0 && "lg:col-span-2")}>
                  <div className="group relative overflow-hidden bg-white">
                    <div className={cn("overflow-hidden", index === 0 ? "aspect-[16/10]" : "aspect-[3/4]")}>
                      <img
                        src={product.image_url ?? exampleImages.productLifestyle}
                        alt={product.name}
                        className="h-full w-full object-cover transition-transform duration-700 group-hover:scale-[1.03]"
                      />
                    </div>
                  </div>
                  <div className="space-y-1 pt-4">
                    <p className="text-[10px] tracking-[0.18em] text-[var(--ex-muted)] uppercase">
                      {product.name}
                    </p>
                    <p className="font-ex-display text-xl">
                      {formatCurrency(product.price_in_cents)}
                    </p>
                  </div>
                </article>
              ))}
            </div>
          )}
        </div>
      </section>

      <section className="mx-auto grid max-w-7xl items-center gap-10 px-4 py-24 md:grid-cols-2 md:px-8">
        <div className="overflow-hidden">
          <img
            src={exampleImages.editorialAracaju}
            alt="Praia em Aracaju"
            className="aspect-[4/5] w-full object-cover"
          />
        </div>
        <div className="space-y-6">
          <p className="text-[10px] tracking-[0.22em] text-[var(--ex-pink)] uppercase">
            Nossa história
          </p>
          <h2 className="font-ex-display text-4xl leading-tight md:text-5xl">
            Moda praia feita com carinho em Aracaju
          </h2>
          <p className="leading-relaxed text-[var(--ex-muted)]">
            A Linda Flor nasceu para vestir mulheres que querem se sentir lindas
            na praia. Qualidade no tecido, modelagem que valoriza o corpo e um
            atendimento próximo de quem entende moda praia de verdade.
          </p>
          <p className="text-[10px] tracking-[0.18em] text-[var(--ex-gold)] uppercase">
            @BIQUINISLINDAFLOR
          </p>
        </div>
      </section>

      <footer className="border-t border-[color-mix(in_oklab,var(--ex-ink)_8%,transparent)] px-4 py-12 md:px-8">
        <div className="mx-auto flex max-w-7xl flex-col items-center justify-between gap-4 text-center md:flex-row md:text-left">
          <span className="font-ex-display text-2xl italic">Linda Flor</span>
          <p className="text-xs text-[var(--ex-muted)]">
            Exemplo Editorial Playfair · Homepage
          </p>
        </div>
      </footer>
    </div>
  );
}
