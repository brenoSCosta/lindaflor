import { db } from "@lindaflor/db";
import { warehouses } from "@lindaflor/db/schema/commerce";
import { ORPCError } from "@orpc/server";
import { asc, eq } from "drizzle-orm";

export const DEFAULT_WAREHOUSE_ID = "0194f0a0-0000-7000-8000-000000000001";

let cachedDefaultWarehouseId: string | null = null;

export async function getDefaultWarehouseId() {
  if (cachedDefaultWarehouseId) {
    return cachedDefaultWarehouseId;
  }

  const [row] = await db
    .select({ id: warehouses.id })
    .from(warehouses)
    .where(eq(warehouses.is_default, true))
    .limit(1);

  if (!row) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Depósito padrão não configurado",
    });
  }

  cachedDefaultWarehouseId = row.id;
  return row.id;
}

export async function listWarehouses() {
  const rows = await db
    .select({
      id: warehouses.id,
      code: warehouses.code,
      name: warehouses.name,
      is_default: warehouses.is_default,
      active: warehouses.active,
    })
    .from(warehouses)
    .orderBy(asc(warehouses.name));

  return { data: rows };
}

export async function createWarehouse(input: { code: string; name: string }) {
  const [created] = await db
    .insert(warehouses)
    .values({
      code: input.code.trim().toLowerCase(),
      name: input.name.trim(),
      is_default: false,
      active: true,
    })
    .returning({
      id: warehouses.id,
      code: warehouses.code,
      name: warehouses.name,
      is_default: warehouses.is_default,
      active: warehouses.active,
    });

  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar depósito",
    });
  }

  return created;
}

export async function ensureDefaultWarehouse() {
  const [existing] = await db
    .select({ id: warehouses.id })
    .from(warehouses)
    .where(eq(warehouses.is_default, true))
    .limit(1);

  if (existing) {
    return existing.id;
  }

  const [created] = await db
    .insert(warehouses)
    .values({
      id: DEFAULT_WAREHOUSE_ID,
      code: "principal",
      name: "Loja Principal",
      is_default: true,
      active: true,
    })
    .onConflictDoNothing()
    .returning({ id: warehouses.id });

  return created?.id ?? DEFAULT_WAREHOUSE_ID;
}
