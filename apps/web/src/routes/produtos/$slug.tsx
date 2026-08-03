import {
  productCategoryLabels,
  productSizeLabels,
} from "@lindaflor/shared/enums/commerce";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useState } from "react";
import { toast } from "sonner";

import { JsonLd } from "@/components/store/json-ld";
import { ProductCard } from "@/components/store/product-card";
import { storeJsonLd } from "@/components/store/store-json-ld";
import { StoreLayout } from "@/components/store/store-layout";
import { Button } from "@/components/ui/button";
import { useCart } from "@/context/cart";
import { formatCurrency } from "@/lib/format";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";

export const Route = createFileRoute("/produtos/$slug")({
  component: ProductDetailPage,
  head: ({ params }) => ({
    meta: [
      { title: `${params.slug.replace(/-/g, " ")} — Linda Flor` },
      {
        name: "description",
        content: "Moda praia Linda Flor — biquínis, maiôs e saídas de praia.",
      },
      { property: "og:type", content: "product" },
    ],
  }),
});

function ProductDetailPage() {
  const { slug } = Route.useParams();
  const { addItem } = useCart();
  const [selectedVariantId, setSelectedVariantId] = useState<string | null>(
    null,
  );
  const [selectedImageUrl, setSelectedImageUrl] = useState<string | null>(null);

  const productQuery = useQuery(
    orpc.commerce.store.getProduct.queryOptions({ input: { slug } }),
  );

  const relatedQuery = useQuery({
    ...orpc.commerce.store.listProducts.queryOptions({
      input: { category: productQuery.data?.category },
    }),
    enabled: Boolean(productQuery.data?.category),
  });

  if (productQuery.isLoading) {
    return (
      <StoreLayout>
        <div className="mx-auto max-w-7xl px-4 py-24 text-center text-(--lf-muted) md:px-8">
          Carregando produto…
        </div>
      </StoreLayout>
    );
  }

  if (productQuery.isError || !productQuery.data) {
    return (
      <StoreLayout>
        <div className="mx-auto max-w-7xl px-4 py-24 text-center md:px-8">
          <p className="font-display text-3xl">Produto não encontrado</p>
          <Link to="/produtos" className="mt-6 inline-block">
            <Button className="rounded-none bg-(--lf-pink) uppercase">
              Voltar ao catálogo
            </Button>
          </Link>
        </div>
      </StoreLayout>
    );
  }

  const product = productQuery.data;
  const selectedVariant =
    product.variants.find((variant) => variant.id === selectedVariantId) ??
    product.variants.find((variant) => variant.available > 0) ??
    product.variants[0];

  const price = selectedVariant?.price_in_cents ?? product.price_in_cents;
  const mainImage =
    selectedImageUrl ?? product.image_url ?? product.images[0]?.url ?? null;

  const relatedProducts =
    relatedQuery.data?.data
      .filter((item) => item.slug !== product.slug)
      .slice(0, 4) ?? [];

  function handleAddToCart() {
    if (!selectedVariant || selectedVariant.available === 0) {
      return;
    }

    addItem(
      {
        variantId: selectedVariant.id,
        productId: product.id,
        productSlug: product.slug,
        productName: product.name,
        variantLabel: `${productSizeLabels[selectedVariant.size]} · ${selectedVariant.color}`,
        imageUrl: mainImage,
        unitPriceCents: price,
        maxQuantity: selectedVariant.available,
      },
      1,
    );
    toast.success("Adicionado ao carrinho");
  }

  return (
    <StoreLayout>
      <JsonLd
        data={storeJsonLd.product({
          name: product.name,
          description: product.description,
          slug: product.slug,
          imageUrl: mainImage,
          priceCents: price,
          inStock: product.available_total > 0,
        })}
      />
      <main className="mx-auto max-w-7xl px-4 py-10 md:px-8 md:py-16">
        <div className="mb-8 text-[11px] tracking-[0.18em] text-(--lf-muted) uppercase">
          <Link to="/" className="hover:text-(--lf-pink)">
            Início
          </Link>
          <span className="mx-2">/</span>
          <Link to="/produtos" className="hover:text-(--lf-pink)">
            Catálogo
          </Link>
          <span className="mx-2">/</span>
          <span>{product.name}</span>
        </div>

        <div className="grid gap-12 lg:grid-cols-2 lg:gap-16">
          <div className="space-y-4">
            <div className="overflow-hidden bg-(--lf-cream-dark)">
              <div className="aspect-3/4">
                {mainImage ? (
                  <img
                    src={mainImage}
                    alt={product.name}
                    className="h-full w-full object-cover"
                  />
                ) : null}
              </div>
            </div>
            {product.images.length > 1 ? (
              <div className="grid grid-cols-4 gap-3">
                {product.images.map((image) => (
                  <button
                    key={image.id}
                    type="button"
                    onClick={() => setSelectedImageUrl(image.url)}
                    className={cn(
                      "aspect-square overflow-hidden bg-(--lf-cream-dark) ring-offset-2",
                      mainImage === image.url && "ring-2 ring-(--lf-pink)",
                    )}
                  >
                    <img
                      src={image.url}
                      alt={image.alt ?? product.name}
                      className="h-full w-full object-cover"
                    />
                  </button>
                ))}
              </div>
            ) : null}
          </div>

          <div className="space-y-8 lg:pt-8">
            <div className="space-y-4">
              <p className="text-[11px] tracking-[0.22em] text-(--lf-pink) uppercase">
                {productCategoryLabels[product.category]}
              </p>
              <h1 className="font-display text-5xl leading-tight text-(--lf-ink) md:text-6xl">
                {product.name}
              </h1>
              <p className="text-2xl font-medium text-(--lf-pink)">
                {formatCurrency(price)}
              </p>
              <p className="max-w-lg leading-relaxed text-(--lf-muted)">
                {product.description}
              </p>
            </div>

            <div className="space-y-4">
              <p className="text-[11px] tracking-[0.18em] uppercase">
                Selecione tamanho e cor
              </p>
              <div className="grid gap-2">
                {product.variants.map((variant) => {
                  const isSelected = selectedVariant?.id === variant.id;
                  const disabled = variant.available === 0;

                  return (
                    <button
                      key={variant.id}
                      type="button"
                      disabled={disabled}
                      onClick={() => setSelectedVariantId(variant.id)}
                      className={cn(
                        "flex items-center justify-between border px-4 py-4 text-left transition-colors",
                        isSelected
                          ? "border-(--lf-pink) bg-[color-mix(in_oklab,var(--lf-pink)_8%,transparent)]"
                          : "border-(--lf-line) bg-transparent",
                        disabled && "cursor-not-allowed opacity-45",
                      )}
                    >
                      <div>
                        <p className="font-medium">
                          {productSizeLabels[variant.size]} · {variant.color}
                        </p>
                        <p className="text-xs text-(--lf-muted)">
                          SKU {variant.sku}
                        </p>
                      </div>
                      <p className="text-sm">
                        {variant.available > 0
                          ? `${variant.available} disponíveis`
                          : "Esgotado"}
                      </p>
                    </button>
                  );
                })}
              </div>
            </div>

            <div className="space-y-3">
              <Button
                className="h-14 w-full rounded-none bg-(--lf-pink) text-[11px] tracking-[0.2em] uppercase hover:bg-(--lf-pink-deep)"
                disabled={!selectedVariant || selectedVariant.available === 0}
                onClick={handleAddToCart}
              >
                {selectedVariant && selectedVariant.available > 0
                  ? "Adicionar ao carrinho"
                  : "Indisponível"}
              </Button>
              <a
                href={`https://wa.me/5579998165115?text=${encodeURIComponent(`Olá! Tenho interesse no ${product.name}${selectedVariant ? ` (${productSizeLabels[selectedVariant.size]} · ${selectedVariant.color})` : ""}`)}`}
                target="_blank"
                rel="noreferrer"
                className="block"
              >
                <Button
                  variant="outline"
                  className="h-14 w-full rounded-none border-(--lf-line-rose) text-[11px] tracking-[0.2em] uppercase"
                >
                  Comprar pelo WhatsApp
                </Button>
              </a>
            </div>

            <div className="space-y-3 border-t border-(--lf-line) pt-6 text-sm">
              <details className="group">
                <summary className="cursor-pointer text-[11px] tracking-[0.18em] uppercase">
                  Frete e entrega
                </summary>
                <p className="mt-2 text-muted">
                  Frete grátis para compras acima de R$ 299. Para Sergipe, prazo
                  de 3 a 7 dias úteis. Demais estados, 7 a 15 dias úteis.
                </p>
              </details>
              <details className="group">
                <summary className="cursor-pointer text-[11px] tracking-[0.18em] uppercase">
                  Trocas e devoluções
                </summary>
                <p className="mt-2 text-muted">
                  Você tem até 7 dias após o recebimento para solicitar troca ou
                  devolução. Consulte nossa{" "}
                  <Link
                    to="/trocas-devolucoes"
                    className="text-(--lf-pink) underline"
                  >
                    política completa
                  </Link>
                  .
                </p>
              </details>
              <details className="group">
                <summary className="cursor-pointer text-[11px] tracking-[0.18em] uppercase">
                  Cuidados com a peça
                </summary>
                <p className="mt-2 text-muted">
                  Lave à mão com água fria. Não use alvejante. Seque à sombra
                  para preservar cores e elasticidade.
                </p>
              </details>
            </div>
          </div>
        </div>

        {relatedProducts.length > 0 ? (
          <section className="mt-24 border-t border-(--lf-line) pt-16">
            <p className="text-[10px] tracking-[0.22em] text-(--lf-pink) uppercase">
              Você também pode gostar
            </p>
            <h2 className="font-display mt-2 mb-10 text-4xl text-(--lf-ink)">
              Complete o look
            </h2>
            <div className="grid gap-8 sm:grid-cols-2 lg:grid-cols-4">
              {relatedProducts.map((item) => (
                <ProductCard key={item.id} product={item} variant="editorial" />
              ))}
            </div>
          </section>
        ) : null}
      </main>
    </StoreLayout>
  );
}
