import { listStoreProducts } from "@lindaflor/core/commerce/products";
import { db } from "@lindaflor/db";
import { collections } from "@lindaflor/db/schema/commerce";
import { schema } from "@lindaflor/shared/schemas/commerce";
import { ORPCError } from "@orpc/server";
import { and, asc, eq } from "drizzle-orm";

export async function listStoreCollections() {
  const rows = await db
    .select({
      id: collections.id,
      name: collections.name,
      slug: collections.slug,
      description: collections.description,
    })
    .from(collections)
    .where(eq(collections.active, true))
    .orderBy(asc(collections.name));

  const withCounts = await Promise.all(
    rows.map(async (collection) => {
      const { data } = await listStoreProducts({
        collection_slug: collection.slug,
      });
      return Object.assign({}, collection, { product_count: data.length });
    }),
  );

  return {
    data: schema.store.listCollections.output.shape.data.parse(withCounts),
  };
}

export async function getStoreCollectionBySlug(slug: string) {
  const [collection] = await db
    .select()
    .from(collections)
    .where(and(eq(collections.slug, slug), eq(collections.active, true)))
    .limit(1);

  if (!collection) {
    throw new ORPCError("NOT_FOUND", { message: "Coleção não encontrada" });
  }

  const { data } = await listStoreProducts({ collection_slug: slug });

  return schema.store.getCollection.output.parse({
    id: collection.id,
    name: collection.name,
    slug: collection.slug,
    description: collection.description,
    products: data,
  });
}
