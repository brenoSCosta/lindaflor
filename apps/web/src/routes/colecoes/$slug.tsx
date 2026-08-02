import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import { ProductCard } from "@/components/store/product-card";
import { StoreLayout } from "@/components/store/store-layout";
import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/colecoes/$slug")({
  component: CollectionDetailPage,
});

function CollectionDetailPage() {
  const { slug } = Route.useParams();

  const collectionQuery = useQuery(
    orpc.commerce.store.getCollection.queryOptions({ input: { slug } }),
  );

  if (collectionQuery.isLoading) {
    return (
      <StoreLayout headerVariant="solid">
        <div className="px-4 py-24 text-center text-[var(--lf-muted)]">
          Carregando coleção...
        </div>
      </StoreLayout>
    );
  }

  if (collectionQuery.isError || !collectionQuery.data) {
    return (
      <StoreLayout headerVariant="solid">
        <div className="px-4 py-24 text-center">Coleção não encontrada.</div>
      </StoreLayout>
    );
  }

  const collection = collectionQuery.data;

  return (
    <StoreLayout headerVariant="solid">
      <main className="mx-auto max-w-7xl px-4 py-16 md:px-8">
        <h1 className="font-display text-5xl">{collection.name}</h1>
        {collection.description ? (
          <p className="mt-3 max-w-2xl text-[var(--lf-muted)]">
            {collection.description}
          </p>
        ) : null}
        <div className="mt-12 grid gap-8 sm:grid-cols-2 lg:grid-cols-3">
          {collection.products.map((product) => (
            <ProductCard key={product.id} product={product} variant="editorial" />
          ))}
        </div>
      </main>
    </StoreLayout>
  );
}
