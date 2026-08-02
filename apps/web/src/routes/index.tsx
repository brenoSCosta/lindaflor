import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { ArrowRight } from "lucide-react";

import { JsonLd, storeJsonLd } from "@/components/store/json-ld";
import { ProductCard } from "@/components/store/product-card";
import { StoreLayout } from "@/components/store/store-layout";
import { Button } from "@/components/ui/button";
import { storeImages } from "@/lib/store-images";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/")({
  component: HomePage,
  head: () => ({
    meta: [
      { title: "Linda Flor — Moda Praia" },
      {
        name: "description",
        content:
          "Biquínis, maiôs e saídas de praia. Moda praia feminina com estilo tropical.",
      },
    ],
  }),
});

const categories = [
  {
    label: "Biquínis",
    to: "/produtos" as const,
    image: storeImages.categoryBiquini,
  },
  {
    label: "Maiôs",
    to: "/produtos" as const,
    image: storeImages.categoryMaio,
  },
  {
    label: "Saídas de praia",
    to: "/produtos" as const,
    image: storeImages.categorySaida,
  },
] as const;

function HomePage() {
  const featuredQuery = useQuery(
    orpc.commerce.store.listProducts.queryOptions({
      input: { featured: true },
    }),
  );
  const allProductsQuery = useQuery(
    orpc.commerce.store.listProducts.queryOptions({ input: {} }),
  );

  const featured = featuredQuery.data?.data ?? [];
  const allProducts = allProductsQuery.data?.data ?? [];

  return (
    <StoreLayout>
      <JsonLd
        data={[storeJsonLd.organization(), storeJsonLd.website()]}
      />
      <section className="relative min-h-[85vh] overflow-hidden">
        <img
          src={storeImages.heroBeach}
          alt=""
          className="absolute inset-0 h-full w-full object-cover"
        />
        <div className="absolute inset-0 bg-gradient-to-t from-[color-mix(in_oklab,var(--lf-ink)_55%,transparent)] via-[color-mix(in_oklab,var(--lf-ink)_10%,transparent)] to-transparent" />
        <div className="relative mx-auto flex min-h-[85vh] max-w-7xl items-end px-4 pb-16 md:px-8 md:pb-24">
          <div className="max-w-xl space-y-6 text-white">
            <p className="text-[10px] tracking-[0.24em] text-white/75 uppercase">
              Coleção Verão 2026
            </p>
            <h1 className="font-display text-5xl leading-[0.95] md:text-7xl lg:text-8xl">
              Elegância tropical
            </h1>
            <p className="max-w-md text-sm leading-relaxed text-white/80 md:text-base">
              Peças pensadas para quem ama sol, mar e estilo. Do biquíni clássico
              à saída de praia perfeita — tudo com a assinatura Linda Flor.
            </p>
            <div className="flex flex-wrap gap-3 pt-2">
              <Link to="/produtos">
                <Button className="h-12 rounded-none bg-[var(--lf-pink)] px-8 text-[10px] tracking-[0.2em] text-white uppercase hover:bg-[var(--lf-pink-deep)]">
                  Comprar agora
                  <ArrowRight className="ml-2 size-4" />
                </Button>
              </Link>
              <Link to="/produtos" hash="colecoes">
                <Button
                  variant="outline"
                  className="h-12 rounded-none border-white/50 bg-transparent px-8 text-[10px] tracking-[0.2em] text-white uppercase backdrop-blur-sm hover:bg-white/10 hover:text-white"
                >
                  Ver coleções
                </Button>
              </Link>
            </div>
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-7xl px-4 py-20 md:px-8">
        <div className="mb-10">
          <p className="text-[10px] tracking-[0.22em] text-[var(--lf-pink)] uppercase">
            Categorias
          </p>
          <h2 className="font-display mt-2 text-4xl text-[var(--lf-ink)] md:text-5xl">
            Explore a coleção
          </h2>
        </div>
        <div className="grid gap-4 md:grid-cols-3">
          {categories.map((category) => (
            <Link
              key={category.label}
              to={category.to}
              className="group relative aspect-[4/5] overflow-hidden"
            >
              <img
                src={category.image}
                alt={category.label}
                className="h-full w-full object-cover transition-transform duration-700 group-hover:scale-105"
              />
              <div className="absolute inset-0 bg-gradient-to-t from-[color-mix(in_oklab,var(--lf-ink)_50%,transparent)] to-transparent" />
              <p className="absolute bottom-6 left-6 font-display text-3xl text-white">
                {category.label}
              </p>
            </Link>
          ))}
        </div>
      </section>

      <section className="border-y border-[color-mix(in_oklab,var(--lf-ink)_6%,transparent)] bg-[var(--lf-cream-dark)] px-4 py-20 md:px-8">
        <div className="mx-auto max-w-7xl">
          <div className="mb-12 flex items-end justify-between gap-6">
            <div>
              <p className="text-[10px] tracking-[0.22em] text-[var(--lf-pink)] uppercase">
                Destaques
              </p>
              <h2 className="font-display mt-2 text-4xl text-[var(--lf-ink)] md:text-5xl">
                Os favoritos da temporada
              </h2>
            </div>
            <Link
              to="/produtos"
              className="hidden items-center gap-2 text-[10px] tracking-[0.18em] uppercase md:inline-flex"
            >
              Ver tudo
              <ArrowRight className="size-4" />
            </Link>
          </div>

          {featuredQuery.isLoading ? (
            <p className="text-[var(--lf-muted)]">Carregando destaques...</p>
          ) : (
            <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
              {featured.map((product, index) => (
                <ProductCard
                  key={product.id}
                  product={product}
                  variant="editorial"
                  aspect={index === 0 ? "wide" : "portrait"}
                  className={cn(index === 0 && "lg:col-span-2")}
                />
              ))}
            </div>
          )}
        </div>
      </section>

      <section
        id="colecoes"
        className="mx-auto grid max-w-7xl items-center gap-10 px-4 py-24 md:grid-cols-2 md:px-8"
      >
        <div className="overflow-hidden">
          <img
            src={storeImages.editorialAracaju}
            alt="Praia em Aracaju"
            className="aspect-[4/5] w-full object-cover"
          />
        </div>
        <div className="space-y-6">
          <p className="text-[10px] tracking-[0.22em] text-[var(--lf-pink)] uppercase">
            Nossa história
          </p>
          <h2 className="font-display text-4xl leading-tight text-[var(--lf-ink)] md:text-5xl">
            Moda praia feita com carinho em Aracaju
          </h2>
          <p className="leading-relaxed text-[var(--lf-muted)]">
            A Linda Flor nasceu para vestir mulheres que querem se sentir lindas
            na praia, na piscina e no pôr do sol. Qualidade no tecido, modelagem
            que valoriza o corpo e um atendimento próximo de quem entende moda
            praia de verdade.
          </p>
          <p className="text-[10px] tracking-[0.18em] text-[var(--lf-gold)] uppercase">
            @BIQUINISLINDAFLOR
          </p>
        </div>
      </section>

      <section id="sobre" className="mx-auto max-w-7xl px-4 py-20 md:px-8">
        <div className="mb-12 text-center">
          <p className="text-[10px] tracking-[0.22em] text-[var(--lf-pink)] uppercase">
            Shop all
          </p>
          <h2 className="font-display mt-2 text-4xl text-[var(--lf-ink)] md:text-5xl">
            Todo o catálogo
          </h2>
          <p className="mx-auto mt-3 max-w-2xl text-[var(--lf-muted)]">
            Para quem sabe que é verão o ano inteiro — encontre sua próxima peça
            favorita.
          </p>
        </div>

        {allProductsQuery.isLoading ? (
          <p className="text-center text-[var(--lf-muted)]">Carregando...</p>
        ) : (
          <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
            {allProducts.map((product) => (
              <ProductCard
                key={product.id}
                product={product}
                variant="editorial"
              />
            ))}
          </div>
        )}
      </section>

      <section className="border-t border-[color-mix(in_oklab,var(--lf-ink)_8%,transparent)] bg-[var(--lf-ink)] px-4 py-16 text-center text-[var(--lf-cream)] md:px-8">
        <p className="text-[10px] tracking-[0.24em] uppercase">
          Atendimento personalizado
        </p>
        <h2 className="font-display mt-3 text-4xl md:text-5xl">
          Fale com a gente no WhatsApp
        </h2>
        <p className="mx-auto mt-4 max-w-xl text-sm leading-relaxed text-white/70">
          Tire dúvidas sobre tamanhos, cores e disponibilidade. Estamos em
          Aracaju, de segunda a sábado, das 8h às 18h.
        </p>
        <a
          href="https://wa.me/5579998165115"
          target="_blank"
          rel="noreferrer"
          className="mt-8 inline-flex"
        >
          <Button className="h-12 rounded-none bg-[var(--lf-cream)] px-8 text-[10px] tracking-[0.2em] text-[var(--lf-ink)] uppercase hover:bg-white">
            Chamar no WhatsApp
          </Button>
        </a>
      </section>
    </StoreLayout>
  );
}
