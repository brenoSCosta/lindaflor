import { db } from "@lindaflor/db";
import {
  collections,
  inventory,
  product_images,
  product_variants,
  products,
} from "@lindaflor/db/schema/commerce";
import { env } from "@lindaflor/env/server";

import { ensureDefaultWarehouse } from "@lindaflor/core/commerce/warehouses";

import { randomStockQuantity } from "@/seed/seeders/randomize-inventory";

function localProductImage(path: string) {
  return `${env.WEB_ORIGIN.replace(/\/$/, "")}${path}`;
}

const productSeedImages = {
  biquiniCanelado: localProductImage("/products/biquini-canelado-rosa.jpg"),
  biquiniTropical: localProductImage("/products/biquini-tropical-verde.jpg"),
  maioPreto: localProductImage("/products/maio-recorte-preto.jpg"),
  saidaCroche: localProductImage("/products/saida-praia-croche.jpg"),
} as const;

export async function seedCommerce() {
  const warehouseId = await ensureDefaultWarehouse();

  const [collection] = await db
    .insert(collections)
    .values({
      name: "Verão 2026",
      slug: "verao-2026",
      description: "Coleção de biquínis para o verão",
      active: true,
    })
    .returning();

  if (!collection) {
    throw new Error("Failed to seed collection");
  }

  const catalog = [
    {
      name: "Biquíni Canelado Rosa",
      slug: "biquini-canelado-rosa",
      description:
        "Top e calcinha em malha canelada com acabamento macio. Ideal para dias de sol.",
      price_in_cents: 18990,
      category: "biquini" as const,
      featured: true,
      image: productSeedImages.biquiniCanelado,
      variants: [
        { sku: "BCR-PP-ROSA", size: "pp" as const, color: "Rosa" },
        { sku: "BCR-P-ROSA", size: "p" as const, color: "Rosa" },
        { sku: "BCR-M-ROSA", size: "m" as const, color: "Rosa" },
        { sku: "BCR-G-ROSA", size: "g" as const, color: "Rosa" },
      ],
    },
    {
      name: "Biquíni Tropical Verde",
      slug: "biquini-tropical-verde",
      description:
        "Estampa tropical com forro interno e bojo removível no top.",
      price_in_cents: 21990,
      category: "biquini" as const,
      featured: true,
      image: productSeedImages.biquiniTropical,
      variants: [
        { sku: "BTV-P-VERDE", size: "p" as const, color: "Verde" },
        { sku: "BTV-M-VERDE", size: "m" as const, color: "Verde" },
        { sku: "BTV-G-VERDE", size: "g" as const, color: "Verde" },
      ],
    },
    {
      name: "Maiô Recorte Preto",
      slug: "maio-recorte-preto",
      description: "Maiô com recortes laterais e tecido com proteção UV.",
      price_in_cents: 24990,
      category: "maio" as const,
      featured: false,
      image: productSeedImages.maioPreto,
      variants: [
        { sku: "MRP-P-PRETO", size: "p" as const, color: "Preto" },
        { sku: "MRP-M-PRETO", size: "m" as const, color: "Preto" },
        { sku: "MRP-G-PRETO", size: "g" as const, color: "Preto" },
      ],
    },
    {
      name: "Saída de Praia Crochê",
      slug: "saida-praia-croche",
      description: "Saída leve em crochê artesanal, perfeita sobre o biquíni.",
      price_in_cents: 15990,
      category: "saida_praia" as const,
      featured: false,
      image: productSeedImages.saidaCroche,
      variants: [
        { sku: "SPC-U-CREME", size: "m" as const, color: "Creme" },
      ],
    },
  ];

  for (const item of catalog) {
    const [product] = await db
      .insert(products)
      .values({
        name: item.name,
        slug: item.slug,
        description: item.description,
        price_in_cents: item.price_in_cents,
        category: item.category,
        collection_id: collection.id,
        featured: item.featured,
        active: true,
      })
      .returning();

    if (!product) {
      throw new Error(`Failed to seed product ${item.slug}`);
    }

    await db.insert(product_images).values({
      product_id: product.id,
      url: item.image,
      alt: item.name,
      sort_order: 0,
    });

    for (const variant of item.variants) {
      const [createdVariant] = await db
        .insert(product_variants)
        .values({
          product_id: product.id,
          sku: variant.sku,
          size: variant.size,
          color: variant.color,
        })
        .returning();

      if (!createdVariant) {
        throw new Error(`Failed to seed variant ${variant.sku}`);
      }

      await db.insert(inventory).values({
        variant_id: createdVariant.id,
        warehouse_id: warehouseId,
        quantity: randomStockQuantity(),
      });
    }
  }
}
