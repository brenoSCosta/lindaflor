import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";

import { StoreLayout } from "@/components/store/store-layout";

import { orpc } from "@/lib/orpc";

export const Route = createFileRoute("/colecoes/")({
  component: CollectionsPage,
});

function CollectionsPage() {
  const collectionsQuery = useQuery(
    orpc.commerce.store.listCollections.queryOptions({ input: undefined }),
  );

  return (
    <StoreLayout headerVariant="solid">
      <main className="mx-auto max-w-7xl px-4 py-16 md:px-8">
        <h1 className="font-display text-5xl">Coleções</h1>
        <p className="mt-3 max-w-xl text-[var(--lf-muted)]">
          Descubra as linhas da Linda Flor, pensadas para cada momento do seu
          verão.
        </p>
        <div className="mt-12 grid gap-6 md:grid-cols-2">
          {collectionsQuery.data?.data.map((collection) => (
            <Link
              key={collection.id}
              to="/colecoes/$slug"
              params={{ slug: collection.slug }}
              className="border border-[var(--lf-line)] p-8 transition-colors hover:border-[var(--lf-pink)]"
            >
              <p className="text-[10px] tracking-[0.2em] text-[var(--lf-pink)] uppercase">
                {collection.product_count} peças
              </p>
              <h2 className="font-display mt-2 text-3xl">{collection.name}</h2>
              {collection.description ? (
                <p className="mt-2 text-sm text-[var(--lf-muted)]">
                  {collection.description}
                </p>
              ) : null}
            </Link>
          ))}
        </div>
      </main>
    </StoreLayout>
  );
}
