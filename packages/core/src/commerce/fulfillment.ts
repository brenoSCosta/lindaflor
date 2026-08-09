import { db } from "@lindaflor/db";
import type { Database } from "@lindaflor/db";
import {
  inventory,
  product_variants,
  products,
  warehouses,
} from "@lindaflor/db/schema/commerce";
import { ORPCError } from "@orpc/server";
import { and, asc, desc, eq, inArray, sql } from "drizzle-orm";

export type FulfillmentLine = {
  variant_id: string;
  quantity: number;
  warehouse_id: string;
};

type DbLike = Pick<Database, "select">;

async function listActiveWarehouses(tx: DbLike) {
  return tx
    .select({
      id: warehouses.id,
      is_default: warehouses.is_default,
      name: warehouses.name,
    })
    .from(warehouses)
    .where(eq(warehouses.active, true))
    .orderBy(desc(warehouses.is_default), asc(warehouses.name));
}

async function loadAvailability(
  tx: DbLike,
  variantIds: string[],
  warehouseIds: string[],
) {
  if (variantIds.length === 0 || warehouseIds.length === 0) {
    return new Map<string, number>();
  }

  const rows = await tx
    .select({
      variant_id: inventory.variant_id,
      warehouse_id: inventory.warehouse_id,
      available: sql<number>`greatest(${inventory.quantity} - ${inventory.reserved}, 0)::int`,
    })
    .from(inventory)
    .where(
      and(
        inArray(inventory.variant_id, variantIds),
        inArray(inventory.warehouse_id, warehouseIds),
      ),
    );

  const map = new Map<string, number>();
  for (const row of rows) {
    map.set(`${row.variant_id}:${row.warehouse_id}`, row.available);
  }
  return map;
}

function warehouseCanFulfill(
  warehouseId: string,
  items: Array<{ variant_id: string; quantity: number }>,
  availability: Map<string, number>,
) {
  return items.every(
    (item) =>
      (availability.get(`${item.variant_id}:${warehouseId}`) ?? 0) >=
      item.quantity,
  );
}

/**
 * Prefer a single warehouse that can fulfill the whole cart (default first).
 * Fall back to per-line allocation: default warehouse if available, else
 * warehouse with highest available stock for that variant.
 */
export async function allocateFulfillment(
  items: Array<{ variant_id: string; quantity: number }>,
  tx: DbLike = db,
): Promise<FulfillmentLine[]> {
  const uniqueVariantIds = [...new Set(items.map((item) => item.variant_id))];
  const activeWarehouses = await listActiveWarehouses(tx);

  if (activeWarehouses.length === 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Nenhum depósito ativo configurado",
    });
  }

  const warehouseIds = activeWarehouses.map((warehouse) => warehouse.id);
  const availability = await loadAvailability(
    tx,
    uniqueVariantIds,
    warehouseIds,
  );

  for (const warehouse of activeWarehouses) {
    if (warehouseCanFulfill(warehouse.id, items, availability)) {
      return items.map((item) => ({
        variant_id: item.variant_id,
        quantity: item.quantity,
        warehouse_id: warehouse.id,
      }));
    }
  }

  const variantNames = await tx
    .select({
      id: product_variants.id,
      product_name: products.name,
    })
    .from(product_variants)
    .innerJoin(products, eq(products.id, product_variants.product_id))
    .where(inArray(product_variants.id, uniqueVariantIds));

  const nameByVariantId = new Map(
    variantNames.map((row) => [row.id, row.product_name] as const),
  );

  const defaultWarehouse = activeWarehouses.find((w) => w.is_default);
  const allocated: FulfillmentLine[] = [];

  for (const item of items) {
    let chosenWarehouseId: string | null = null;

    if (defaultWarehouse) {
      const available =
        availability.get(`${item.variant_id}:${defaultWarehouse.id}`) ?? 0;
      if (available >= item.quantity) {
        chosenWarehouseId = defaultWarehouse.id;
      }
    }

    if (!chosenWarehouseId) {
      let bestAvailable = -1;
      for (const warehouse of activeWarehouses) {
        const available =
          availability.get(`${item.variant_id}:${warehouse.id}`) ?? 0;
        if (available >= item.quantity && available > bestAvailable) {
          bestAvailable = available;
          chosenWarehouseId = warehouse.id;
        }
      }
    }

    if (!chosenWarehouseId) {
      const productName = nameByVariantId.get(item.variant_id) ?? "Produto";
      throw new ORPCError("BAD_REQUEST", {
        message: `${productName} não tem estoque suficiente`,
      });
    }

    allocated.push({
      variant_id: item.variant_id,
      quantity: item.quantity,
      warehouse_id: chosenWarehouseId,
    });
  }

  return allocated;
}
