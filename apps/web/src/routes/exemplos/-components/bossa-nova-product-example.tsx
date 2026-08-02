import { useQuery } from "@tanstack/react-query";
import {
  ChevronDown,
  MessageCircle,
  RefreshCw,
  Search,
  ShoppingBag,
  Truck,
  User,
} from "lucide-react";
import { useState } from "react";

import {
  productCategoryLabels,
  productSizeLabels,
  productSizes,
} from "@lindaflor/shared/enums/commerce";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";
import { exampleImages } from "@/routes/exemplos/-components/example-images";
import { cn } from "@/lib/utils";

const galleryImages = [
  exampleImages.productLifestyle,
  exampleImages.productBack,
  exampleImages.productFlatLay,
  exampleImages.productDetail,
] as const;

const accordions = [
  {
    title: "Detalhes do produto",
    body: "Tecido com proteção UV, forro interno e acabamento premium. Modelagem pensada para conforto e sustentação.",
  },
  {
    title: "Cuidados",
    body: "Lavar à mão com água fria. Não usar alvejante. Secar à sombra para preservar as cores.",
  },
  {
    title: "Entrega e devolução",
    body: "Envio para todo o Brasil em até 7 dias úteis. Troca fácil em até 30 dias.",
  },
] as const;

export function BossaNovaProductExample() {
  const [selectedSize, setSelectedSize] = useState<string>("m");
  const [mainImage, setMainImage] = useState(0);
  const [openAccordion, setOpenAccordion] = useState<string | null>(
    "Detalhes do produto",
  );

  const productQuery = useQuery(
    orpc.commerce.store.getProduct.queryOptions({
      input: { slug: "biquini-tropical-verde" },
    }),
  );
  const relatedQuery = useQuery(
    orpc.commerce.store.listProducts.queryOptions({ input: {} }),
  );

  const product = productQuery.data;
  const related = (relatedQuery.data?.data ?? [])
    .filter((item) => item.slug !== "biquini-tropical-verde")
    .slice(0, 4);

  if (productQuery.isLoading) {
    return (
      <div className="store-example-bossa font-ex-body flex min-h-[60vh] items-center justify-center bg-[var(--ex-bg)] text-[var(--ex-muted)]">
        Carregando produto...
      </div>
    );
  }

  if (!product) {
    return (
      <div className="store-example-bossa font-ex-body flex min-h-[60vh] items-center justify-center bg-[var(--ex-bg)] text-[var(--ex-muted)]">
        Produto de exemplo não encontrado. Rode o seed do banco.
      </div>
    );
  }

  const images =
    product.images.length > 0
      ? product.images.map((img) => img.url)
      : [...galleryImages];

  return (
    <div className="store-example-bossa font-ex-body min-h-screen bg-[var(--ex-bg)] text-[var(--ex-ink)]">
      <header className="border-b border-[color-mix(in_oklab,var(--ex-ink)_8%,transparent)]">
        <div className="mx-auto flex h-20 max-w-7xl items-center justify-between px-4 md:px-10">
          <nav className="hidden gap-6 md:flex">
            {["Coleções", "Biquínis", "Maiôs", "Saídas"].map((item, i) => (
              <span
                key={item}
                className={cn(
                  "text-[10px] tracking-[0.18em] uppercase",
                  i === 1 && "text-[var(--ex-pink)]",
                )}
              >
                {item}
              </span>
            ))}
          </nav>
          <p className="font-ex-display text-2xl md:text-3xl">Linda Flor</p>
          <div className="flex items-center gap-4">
            <Search className="size-4" />
            <User className="size-4" />
            <ShoppingBag className="size-4" />
          </div>
        </div>
      </header>

      <main className="mx-auto max-w-7xl px-4 py-8 md:px-10 md:py-12">
        <p className="mb-8 text-[10px] tracking-[0.16em] text-[var(--ex-muted)] uppercase">
          Início / Catálogo / {product.name}
        </p>

        <div className="grid gap-12 lg:grid-cols-[1.15fr_0.85fr] lg:gap-16">
          <div className="space-y-4">
            <div className="overflow-hidden bg-[var(--ex-sand)]">
              <div className="aspect-[3/4]">
                <img
                  src={images[mainImage] ?? product.image_url ?? galleryImages[0]}
                  alt={product.name}
                  className="h-full w-full object-cover"
                />
              </div>
            </div>
            <div className="grid grid-cols-4 gap-3">
              {images.slice(0, 4).map((url, index) => (
                <button
                  key={url}
                  type="button"
                  onClick={() => setMainImage(index)}
                  className={cn(
                    "aspect-square overflow-hidden border-2 bg-[var(--ex-sand)]",
                    mainImage === index
                      ? "border-[var(--ex-pink)]"
                      : "border-transparent",
                  )}
                >
                  <img
                    src={url}
                    alt=""
                    className="h-full w-full object-cover"
                  />
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-8 lg:pt-4">
            <div className="space-y-3">
              <p className="text-[10px] tracking-[0.2em] text-[var(--ex-muted)] uppercase">
                {productCategoryLabels[product.category]}
              </p>
              <h1 className="font-ex-display text-4xl leading-tight md:text-5xl">
                {product.name}
              </h1>
              <p className="text-2xl font-light">
                {formatCurrency(product.price_in_cents)}
              </p>
              <p className="max-w-md leading-relaxed text-[var(--ex-muted)]">
                {product.description}
              </p>
            </div>

            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <p className="text-[10px] tracking-[0.18em] uppercase">
                  Tamanho
                </p>
                <button
                  type="button"
                  className="text-[10px] tracking-[0.12em] text-[var(--ex-muted)] underline underline-offset-4"
                >
                  Guia de tamanhos
                </button>
              </div>
              <div className="flex flex-wrap gap-2">
                {productSizes.map((size) => (
                  <button
                    key={size}
                    type="button"
                    onClick={() => setSelectedSize(size)}
                    className={cn(
                      "flex size-11 items-center justify-center border text-xs tracking-wider uppercase transition-colors",
                      selectedSize === size
                        ? "border-[var(--ex-pink)] bg-[var(--ex-pink)] text-white"
                        : "border-[color-mix(in_oklab,var(--ex-ink)_15%,transparent)] hover:border-[var(--ex-ink)]",
                    )}
                  >
                    {productSizeLabels[size]}
                  </button>
                ))}
              </div>
            </div>

            <div className="space-y-3">
              <p className="text-[10px] tracking-[0.18em] uppercase">Cor</p>
              <div className="flex gap-2">
                {["#2d5a3d", "#d4147a", "#1a1218"].map((color, i) => (
                  <button
                    key={color}
                    type="button"
                    className={cn(
                      "size-8 rounded-full border-2",
                      i === 0 ? "border-[var(--ex-ink)]" : "border-transparent",
                    )}
                    style={{ backgroundColor: color }}
                  />
                ))}
              </div>
            </div>

            <div className="flex items-center gap-4">
              <div className="flex border border-[color-mix(in_oklab,var(--ex-ink)_12%,transparent)]">
                <button type="button" className="px-4 py-3 text-lg">
                  −
                </button>
                <span className="flex w-10 items-center justify-center text-sm">
                  1
                </span>
                <button type="button" className="px-4 py-3 text-lg">
                  +
                </button>
              </div>
              <button
                type="button"
                className="h-12 flex-1 bg-[var(--ex-pink)] text-[10px] tracking-[0.18em] text-white uppercase hover:bg-[var(--ex-pink-deep)]"
              >
                Adicionar ao carrinho
              </button>
            </div>

            <div className="grid gap-3 border-y border-[color-mix(in_oklab,var(--ex-ink)_8%,transparent)] py-6 sm:grid-cols-3">
              {[
                { icon: RefreshCw, label: "Troca fácil" },
                { icon: Truck, label: "Envio para todo Brasil" },
                { icon: MessageCircle, label: "WhatsApp" },
              ].map(({ icon: Icon, label }) => (
                <div
                  key={label}
                  className="flex items-center gap-2 text-xs text-[var(--ex-muted)]"
                >
                  <Icon className="size-4 shrink-0" />
                  {label}
                </div>
              ))}
            </div>

            <div className="space-y-2">
              {accordions.map((item) => {
                const open = openAccordion === item.title;
                return (
                  <div
                    key={item.title}
                    className="border-b border-[color-mix(in_oklab,var(--ex-ink)_8%,transparent)]"
                  >
                    <button
                      type="button"
                      onClick={() =>
                        setOpenAccordion(open ? null : item.title)
                      }
                      className="flex w-full items-center justify-between py-4 text-left text-[11px] tracking-[0.14em] uppercase"
                    >
                      {item.title}
                      <ChevronDown
                        className={cn(
                          "size-4 transition-transform",
                          open && "rotate-180",
                        )}
                      />
                    </button>
                    {open ? (
                      <p className="pb-4 text-sm leading-relaxed text-[var(--ex-muted)]">
                        {item.body}
                      </p>
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>
        </div>

        <section className="mt-24 border-t border-[color-mix(in_oklab,var(--ex-ink)_8%,transparent)] pt-16">
          <h2 className="font-ex-display mb-10 text-center text-3xl md:text-4xl">
            Complete o look
          </h2>
          <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
            {related.map((item) => (
              <article key={item.id}>
                <div className="aspect-[3/4] overflow-hidden bg-[var(--ex-sand)]">
                  <img
                    src={item.image_url ?? exampleImages.categorySaida}
                    alt={item.name}
                    className="h-full w-full object-cover"
                  />
                </div>
                <p className="mt-3 text-center font-ex-display text-lg">
                  {item.name}
                </p>
                <p className="text-center text-sm text-[var(--ex-muted)]">
                  {formatCurrency(item.price_in_cents)}
                </p>
              </article>
            ))}
          </div>
        </section>
      </main>

      <footer className="mt-12 bg-[color-mix(in_oklab,var(--ex-sand)_60%,var(--ex-bg))] px-4 py-12 md:px-10">
        <div className="mx-auto flex max-w-7xl flex-col justify-between gap-4 md:flex-row md:items-center">
          <p className="font-ex-display text-xl">Linda Flor</p>
          <p className="text-xs text-[var(--ex-muted)]">
            Exemplo Bossa Nova · Página de produto
          </p>
        </div>
      </footer>
    </div>
  );
}
