import { resolveImageUrl } from "@lindaflor/core/commerce/product-images";
import { getDefaultWarehouseId } from "@lindaflor/core/commerce/warehouses";
import { db } from "@lindaflor/db";
import {
  inventory,
  product_images,
  product_variants,
  products,
} from "@lindaflor/db/schema/commerce";
import type { ProductDetail } from "@lindaflor/shared/schemas/commerce";
import { schema } from "@lindaflor/shared/schemas/commerce";
import { ORPCError } from "@orpc/server";
import { and, asc, desc, eq, ilike, inArray, sql } from "drizzle-orm";
import type { z } from "zod";

type ListProductsInput = z.infer<typeof schema.store.listProducts.input>;

function formatVariantLabel(size: string, color: string) {
  return `${size.toUpperCase()} · ${color}`;
}

async function getProductAvailabilityByProductId(productIds: string[]) {
  if (productIds.length === 0) {
    return new Map<string, number>();
  }

  const warehouseId = await getDefaultWarehouseId();

  const rows = await db
    .select({
      product_id: product_variants.product_id,
      available_total: sql<number>`coalesce(sum(greatest(${inventory.quantity} - ${inventory.reserved}, 0)), 0)::int`,
    })
    .from(product_variants)
    .innerJoin(
      inventory,
      and(
        eq(inventory.variant_id, product_variants.id),
        eq(inventory.warehouse_id, warehouseId),
      ),
    )
    .where(inArray(product_variants.product_id, productIds))
    .groupBy(product_variants.product_id);

  return new Map(
    rows.map((row) => [row.product_id, row.available_total] as const),
  );
}

async function getPrimaryImageByProductId(productIds: string[]) {
  if (productIds.length === 0) {
    return new Map<string, string>();
  }

  const rows = await db
    .select({
      product_id: product_images.product_id,
      url: product_images.url,
      sort_order: product_images.sort_order,
    })
    .from(product_images)
    .where(inArray(product_images.product_id, productIds))
    .orderBy(asc(product_images.sort_order));

  const map = new Map<string, string>();
  for (const row of rows) {
    if (!map.has(row.product_id)) {
      map.set(row.product_id, row.url);
    }
  }

  return map;
}

export async function listProductSlugsForSitemap() {
  return db
    .select({
      slug: products.slug,
      updated_at: products.updated_at,
    })
    .from(products)
    .where(eq(products.active, true))
    .orderBy(desc(products.updated_at));
}

export async function listStoreProducts(input: ListProductsInput | undefined) {
  const filters = [eq(products.active, true)];

  if (input?.category) {
    filters.push(eq(products.category, input.category));
  }
  if (input?.featured) {
    filters.push(eq(products.featured, true));
  }
  if (input?.search?.trim()) {
    const term = `%${input.search.trim()}%`;
    filters.push(
      sql`(${ilike(products.name, term)} OR ${ilike(products.description, term)} OR EXISTS (
        SELECT 1 FROM product_variants pv
        WHERE pv.product_id = ${products.id}
        AND pv.sku ILIKE ${term}
      ))`,
    );
  }
  if (input?.collection_slug) {
    filters.push(
      sql`EXISTS (
        SELECT 1 FROM collections c
        WHERE c.id = ${products.collection_id}
        AND c.slug = ${input.collection_slug}
        AND c.active = true
      )`,
    );
  }

  const productRows = await db
    .select()
    .from(products)
    .where(and(...filters))
    .orderBy(desc(products.featured), desc(products.created_at));

  const productIds = productRows.map((product) => product.id);
  const [imageByProductId, availabilityByProductId] = await Promise.all([
    getPrimaryImageByProductId(productIds),
    getProductAvailabilityByProductId(productIds),
  ]);

  const data = schema.store.listProducts.output.shape.data.parse(
    (
      await Promise.all(
        productRows.map(async (product) => {
          const imageKey = imageByProductId.get(product.id);
          return {
            id: product.id,
            name: product.name,
            slug: product.slug,
            description: product.description,
            price_in_cents: product.price_in_cents,
            category: product.category,
            featured: product.featured,
            image_url: imageKey ? await resolveImageUrl(imageKey) : null,
            available_total: availabilityByProductId.get(product.id) ?? 0,
          };
        }),
      )
    ).filter(
      (product) =>
        input?.in_stock_only === false || product.available_total > 0,
    ),
  );

  return { data };
}

export async function getStoreProductBySlug(
  slug: string,
): Promise<ProductDetail> {
  const [product] = await db
    .select()
    .from(products)
    .where(and(eq(products.slug, slug), eq(products.active, true)))
    .limit(1);

  if (!product) {
    throw new ORPCError("NOT_FOUND", { message: "Produto não encontrado" });
  }

  const warehouseId = await getDefaultWarehouseId();

  const [images, variants, collection] = await Promise.all([
    db
      .select()
      .from(product_images)
      .where(eq(product_images.product_id, product.id))
      .orderBy(asc(product_images.sort_order)),
    db
      .select({
        id: product_variants.id,
        sku: product_variants.sku,
        size: product_variants.size,
        color: product_variants.color,
        price_in_cents: product_variants.price_in_cents,
        available: sql<number>`coalesce(greatest(${inventory.quantity} - ${inventory.reserved}, 0), 0)::int`,
      })
      .from(product_variants)
      .leftJoin(
        inventory,
        and(
          eq(inventory.variant_id, product_variants.id),
          eq(inventory.warehouse_id, warehouseId),
        ),
      )
      .where(eq(product_variants.product_id, product.id))
      .orderBy(asc(product_variants.size), asc(product_variants.color)),
    product.collection_id
      ? db.query.collections.findFirst({
          where: (table, { eq: eqFn }) => {
            const collectionId = product.collection_id;
            if (!collectionId) {
              return sql`false`;
            }
            return eqFn(table.id, collectionId);
          },
          columns: { id: true, name: true, slug: true },
        })
      : Promise.resolve(null),
  ]);

  const available_total = variants.reduce(
    (sum, variant) => sum + variant.available,
    0,
  );

  const image_url = images[0]?.url
    ? await resolveImageUrl(images[0].url)
    : null;

  const resolvedImages = await Promise.all(
    images.map(async (image) => {
      const url = await resolveImageUrl(image.url);
      return Object.assign({}, image, { url });
    }),
  );

  return schema.store.getProduct.output.parse({
    id: product.id,
    name: product.name,
    slug: product.slug,
    description: product.description,
    price_in_cents: product.price_in_cents,
    category: product.category,
    featured: product.featured,
    image_url,
    available_total,
    images: resolvedImages,
    variants,
    collection: collection ?? null,
  });
}

export async function listAdminProducts() {
  const productRows = await db
    .select()
    .from(products)
    .orderBy(desc(products.created_at));

  const productIds = productRows.map((product) => product.id);

  const [availabilityByProductId, variantCounts] = await Promise.all([
    getProductAvailabilityByProductId(productIds),
    db
      .select({
        product_id: product_variants.product_id,
        variant_count: sql<number>`count(*)::int`,
      })
      .from(product_variants)
      .where(inArray(product_variants.product_id, productIds))
      .groupBy(product_variants.product_id),
  ]);

  const variantCountByProductId = new Map(
    variantCounts.map((row) => [row.product_id, row.variant_count] as const),
  );

  return {
    data: schema.admin.listProducts.output.shape.data.parse(
      productRows.map((product) => ({
        id: product.id,
        name: product.name,
        slug: product.slug,
        description: product.description,
        price_in_cents: product.price_in_cents,
        category: product.category,
        active: product.active,
        featured: product.featured,
        created_at: product.created_at,
        variant_count: variantCountByProductId.get(product.id) ?? 0,
        available_total: availabilityByProductId.get(product.id) ?? 0,
      })),
    ),
  };
}

type CreateProductInput = z.infer<typeof schema.admin.createProduct.input>;

export async function createProduct(input: CreateProductInput) {
  const existing = await db
    .select({ id: products.id })
    .from(products)
    .where(eq(products.slug, input.slug))
    .limit(1);

  if (existing.length > 0) {
    throw new ORPCError("CONFLICT", {
      message: "Já existe um produto com este slug",
    });
  }

  const warehouseId = await getDefaultWarehouseId();

  const product = await db.transaction(async (tx) => {
    const [created] = await tx
      .insert(products)
      .values({
        name: input.name,
        slug: input.slug,
        description: input.description ?? null,
        price_in_cents: input.price_in_cents,
        category: input.category,
        featured: input.featured,
      })
      .returning();

    if (!created) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar produto",
      });
    }

    await Promise.all(
      input.variants.map(async (variant) => {
        const [createdVariant] = await tx
          .insert(product_variants)
          .values({
            product_id: created.id,
            sku: variant.sku,
            size: variant.size,
            color: variant.color,
            price_in_cents: variant.price_in_cents ?? null,
          })
          .returning();

        if (!createdVariant) {
          throw new ORPCError("INTERNAL_SERVER_ERROR", {
            message: "Falha ao criar variante",
          });
        }

        await tx.insert(inventory).values({
          variant_id: createdVariant.id,
          warehouse_id: warehouseId,
          quantity: variant.quantity,
        });
      }),
    );

    return created;
  });

  return getStoreProductBySlug(product.slug);
}

type UpdateProductInput = z.infer<typeof schema.admin.updateProduct.input>;

export async function updateProduct(input: UpdateProductInput) {
  const [existing] = await db
    .select({ id: products.id, slug: products.slug })
    .from(products)
    .where(eq(products.id, input.id))
    .limit(1);

  if (!existing) {
    throw new ORPCError("NOT_FOUND", { message: "Produto não encontrado" });
  }

  if (input.slug !== existing.slug) {
    const conflict = await db
      .select({ id: products.id })
      .from(products)
      .where(eq(products.slug, input.slug))
      .limit(1);

    if (conflict.length > 0) {
      throw new ORPCError("CONFLICT", {
        message: "Já existe um produto com este slug",
      });
    }
  }

  await db
    .update(products)
    .set({
      name: input.name,
      slug: input.slug,
      description: input.description ?? null,
      price_in_cents: input.price_in_cents,
      category: input.category,
      featured: input.featured,
      active: input.active,
    })
    .where(eq(products.id, input.id));

  return getStoreProductBySlug(input.slug);
}

export { formatVariantLabel };
