import { getDefaultWarehouseId } from "@lindaflor/core/commerce/warehouses";
import { db } from "@lindaflor/db";
import { users } from "@lindaflor/db/schema/auth";
import {
  inventory,
  inventory_movements,
  product_variants,
  products,
  warehouses,
} from "@lindaflor/db/schema/commerce";
import { env } from "@lindaflor/env/server";
import { schema } from "@lindaflor/shared/schemas/commerce";
import { ORPCError } from "@orpc/server";
import { and, asc, desc, eq, ilike, sql } from "drizzle-orm";
import type { z } from "zod";

type AdjustInventoryInput = z.infer<typeof schema.admin.adjustInventory.input>;
type ReceiveInventoryInput = z.infer<
  typeof schema.admin.receiveInventory.input
>;
type TransferInventoryInput = z.infer<
  typeof schema.admin.transferInventory.input
>;
type ListMovementsInput = z.infer<
  typeof schema.admin.listInventoryMovements.input
>;
type ImportInventoryInput = z.infer<
  typeof schema.admin.importInventoryCsv.input
>;

function mapInventoryRow(row: {
  variant_id: string;
  product_name: string;
  sku: string;
  size: string;
  color: string;
  warehouse_id: string;
  warehouse_code: string;
  warehouse_name: string;
  quantity: number;
  reserved: number;
  low_stock_threshold: number;
}) {
  return {
    variant_id: row.variant_id,
    product_name: row.product_name,
    sku: row.sku,
    size: row.size,
    color: row.color,
    warehouse_id: row.warehouse_id,
    warehouse_code: row.warehouse_code,
    warehouse_name: row.warehouse_name,
    quantity: row.quantity,
    reserved: row.reserved,
    available: Math.max(row.quantity - row.reserved, 0),
    low_stock_threshold: row.low_stock_threshold,
    is_low_stock:
      Math.max(row.quantity - row.reserved, 0) <= row.low_stock_threshold,
  };
}

function inventorySelect() {
  return db
    .select({
      variant_id: product_variants.id,
      product_name: products.name,
      sku: product_variants.sku,
      size: product_variants.size,
      color: product_variants.color,
      warehouse_id: warehouses.id,
      warehouse_code: warehouses.code,
      warehouse_name: warehouses.name,
      quantity: sql<number>`coalesce(${inventory.quantity}, 0)::int`,
      reserved: sql<number>`coalesce(${inventory.reserved}, 0)::int`,
      low_stock_threshold: product_variants.low_stock_threshold,
    })
    .from(product_variants)
    .innerJoin(products, eq(products.id, product_variants.product_id))
    .innerJoin(warehouses, eq(warehouses.active, true))
    .leftJoin(
      inventory,
      and(
        eq(inventory.variant_id, product_variants.id),
        eq(inventory.warehouse_id, warehouses.id),
      ),
    );
}

export async function listInventory(input?: {
  warehouse_id?: string;
  low_stock_only?: boolean;
}) {
  const filters = [eq(warehouses.active, true)];

  if (input?.warehouse_id) {
    filters.push(eq(warehouses.id, input.warehouse_id));
  }

  const rows = await inventorySelect()
    .where(and(...filters))
    .orderBy(asc(products.name), asc(product_variants.size));

  let data = rows.map(mapInventoryRow);

  if (input?.low_stock_only) {
    data = data.filter((row) => row.is_low_stock);
  }

  return {
    data: schema.admin.listInventory.output.shape.data.parse(data),
  };
}

export async function listLowStockAlerts() {
  const threshold = env.INVENTORY_LOW_STOCK_THRESHOLD;
  const rows = await inventorySelect()
    .where(eq(warehouses.active, true))
    .orderBy(asc(products.name), asc(product_variants.size));

  const data: ReturnType<typeof mapInventoryRow>[] = [];
  for (const row of rows) {
    const mapped = mapInventoryRow(row);
    if (
      mapped.available <= Math.min(mapped.low_stock_threshold, threshold) ||
      mapped.available === 0
    ) {
      data.push(mapped);
    }
  }

  return {
    data: schema.admin.listLowStockAlerts.output.shape.data.parse(data),
    threshold,
  };
}

export async function listInventoryMovements(input: ListMovementsInput) {
  const filters = [];

  if (input.variant_id) {
    filters.push(eq(inventory_movements.variant_id, input.variant_id));
  }
  if (input.warehouse_id) {
    filters.push(eq(inventory_movements.warehouse_id, input.warehouse_id));
  }
  if (input.type) {
    filters.push(eq(inventory_movements.type, input.type));
  }
  if (input.search?.trim()) {
    const term = `%${input.search.trim()}%`;
    filters.push(
      sql`(${ilike(product_variants.sku, term)} OR ${ilike(products.name, term)})`,
    );
  }

  const limit = input.limit ?? 50;
  const offset = input.offset ?? 0;

  const rows = await db
    .select({
      id: inventory_movements.id,
      variant_id: inventory_movements.variant_id,
      product_name: products.name,
      sku: product_variants.sku,
      warehouse_id: inventory_movements.warehouse_id,
      warehouse_name: warehouses.name,
      type: inventory_movements.type,
      quantity: inventory_movements.quantity,
      notes: inventory_movements.notes,
      reference_type: inventory_movements.reference_type,
      reference_id: inventory_movements.reference_id,
      created_by_name: users.name,
      created_at: inventory_movements.created_at,
    })
    .from(inventory_movements)
    .innerJoin(
      product_variants,
      eq(product_variants.id, inventory_movements.variant_id),
    )
    .innerJoin(products, eq(products.id, product_variants.product_id))
    .leftJoin(warehouses, eq(warehouses.id, inventory_movements.warehouse_id))
    .leftJoin(users, eq(users.id, inventory_movements.created_by))
    .where(filters.length > 0 ? and(...filters) : undefined)
    .orderBy(desc(inventory_movements.created_at))
    .limit(limit)
    .offset(offset);

  return {
    data: schema.admin.listInventoryMovements.output.shape.data.parse(rows),
    limit,
    offset,
  };
}

async function applyInventoryChange(params: {
  variant_id: string;
  warehouse_id: string;
  quantity_delta: number;
  type: (typeof inventory_movements.$inferInsert)["type"];
  notes?: string | null;
  reference_type?: string | null;
  reference_id?: string | null;
  created_by: string | null;
}) {
  return db.transaction(async (tx) => {
    const [meta] = await tx
      .select({
        variant_id: product_variants.id,
        product_name: products.name,
        sku: product_variants.sku,
        size: product_variants.size,
        color: product_variants.color,
        warehouse_id: warehouses.id,
        warehouse_code: warehouses.code,
        warehouse_name: warehouses.name,
        low_stock_threshold: product_variants.low_stock_threshold,
      })
      .from(product_variants)
      .innerJoin(products, eq(products.id, product_variants.product_id))
      .innerJoin(warehouses, eq(warehouses.id, params.warehouse_id))
      .where(eq(product_variants.id, params.variant_id))
      .limit(1);

    if (!meta) {
      throw new ORPCError("NOT_FOUND", {
        message: "Variante ou depósito não encontrado",
      });
    }

    const [existing] = await tx
      .select({
        quantity: inventory.quantity,
        reserved: inventory.reserved,
      })
      .from(inventory)
      .where(
        and(
          eq(inventory.variant_id, params.variant_id),
          eq(inventory.warehouse_id, params.warehouse_id),
        ),
      )
      .limit(1);

    const currentQuantity = existing?.quantity ?? 0;
    const currentReserved = existing?.reserved ?? 0;
    const nextQuantity = currentQuantity + params.quantity_delta;

    if (nextQuantity < 0) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Estoque não pode ficar negativo",
      });
    }
    if (nextQuantity < currentReserved) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Estoque não pode ficar abaixo do reservado",
      });
    }

    if (existing) {
      await tx
        .update(inventory)
        .set({ quantity: nextQuantity })
        .where(
          and(
            eq(inventory.variant_id, params.variant_id),
            eq(inventory.warehouse_id, params.warehouse_id),
          ),
        );
    } else {
      await tx.insert(inventory).values({
        variant_id: params.variant_id,
        warehouse_id: params.warehouse_id,
        quantity: nextQuantity,
        reserved: 0,
      });
    }

    await tx.insert(inventory_movements).values({
      variant_id: params.variant_id,
      warehouse_id: params.warehouse_id,
      type: params.type,
      quantity: params.quantity_delta,
      notes: params.notes ?? null,
      reference_type: params.reference_type ?? null,
      reference_id: params.reference_id ?? null,
      created_by: params.created_by,
    });

    return mapInventoryRow({
      ...meta,
      quantity: nextQuantity,
      reserved: currentReserved,
    });
  });
}

export async function adjustInventory(
  input: AdjustInventoryInput,
  createdBy: string | null,
) {
  const warehouseId = input.warehouse_id ?? (await getDefaultWarehouseId());

  const result = await applyInventoryChange({
    variant_id: input.variant_id,
    warehouse_id: warehouseId,
    quantity_delta: input.quantity_delta,
    type: "ajuste",
    notes: input.notes ?? null,
    created_by: createdBy,
  });

  return schema.admin.adjustInventory.output.parse(result);
}

export async function receiveInventory(
  input: ReceiveInventoryInput,
  createdBy: string | null,
) {
  if (input.quantity <= 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Quantidade de entrada deve ser positiva",
    });
  }

  const warehouseId = input.warehouse_id ?? (await getDefaultWarehouseId());

  const result = await applyInventoryChange({
    variant_id: input.variant_id,
    warehouse_id: warehouseId,
    quantity_delta: input.quantity,
    type: "entrada",
    notes: input.notes ?? "Entrada de mercadoria",
    created_by: createdBy,
  });

  return schema.admin.receiveInventory.output.parse(result);
}

export async function transferInventory(
  input: TransferInventoryInput,
  createdBy: string | null,
) {
  if (input.quantity <= 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Quantidade deve ser positiva",
    });
  }
  if (input.from_warehouse_id === input.to_warehouse_id) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Depósitos de origem e destino devem ser diferentes",
    });
  }

  return db.transaction(async (tx) => {
    const ensureRow = async (variantId: string, warehouseId: string) => {
      const [existing] = await tx
        .select({ variant_id: inventory.variant_id })
        .from(inventory)
        .where(
          and(
            eq(inventory.variant_id, variantId),
            eq(inventory.warehouse_id, warehouseId),
          ),
        )
        .limit(1);

      if (!existing) {
        await tx.insert(inventory).values({
          variant_id: variantId,
          warehouse_id: warehouseId,
          quantity: 0,
          reserved: 0,
        });
      }
    };

    await ensureRow(input.variant_id, input.from_warehouse_id);
    await ensureRow(input.variant_id, input.to_warehouse_id);

    const [fromRow] = await tx
      .select({
        quantity: inventory.quantity,
        reserved: inventory.reserved,
      })
      .from(inventory)
      .where(
        and(
          eq(inventory.variant_id, input.variant_id),
          eq(inventory.warehouse_id, input.from_warehouse_id),
        ),
      )
      .limit(1);

    if (!fromRow) {
      throw new ORPCError("NOT_FOUND", {
        message: "Estoque de origem não encontrado",
      });
    }

    const available = fromRow.quantity - fromRow.reserved;
    if (available < input.quantity) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Saldo disponível insuficiente no depósito de origem",
      });
    }

    await tx
      .update(inventory)
      .set({
        quantity: sql`${inventory.quantity} - ${input.quantity}`,
      })
      .where(
        and(
          eq(inventory.variant_id, input.variant_id),
          eq(inventory.warehouse_id, input.from_warehouse_id),
        ),
      );

    await tx
      .update(inventory)
      .set({
        quantity: sql`${inventory.quantity} + ${input.quantity}`,
      })
      .where(
        and(
          eq(inventory.variant_id, input.variant_id),
          eq(inventory.warehouse_id, input.to_warehouse_id),
        ),
      );

    const note = input.notes ?? `Transferência entre depósitos`;

    await tx.insert(inventory_movements).values({
      variant_id: input.variant_id,
      warehouse_id: input.from_warehouse_id,
      type: "transferencia",
      quantity: -input.quantity,
      notes: note,
      created_by: createdBy,
    });

    await tx.insert(inventory_movements).values({
      variant_id: input.variant_id,
      warehouse_id: input.to_warehouse_id,
      type: "transferencia",
      quantity: input.quantity,
      notes: note,
      created_by: createdBy,
    });

    const [updated] = await tx
      .select({
        variant_id: product_variants.id,
        product_name: products.name,
        sku: product_variants.sku,
        size: product_variants.size,
        color: product_variants.color,
        warehouse_id: warehouses.id,
        warehouse_code: warehouses.code,
        warehouse_name: warehouses.name,
        quantity: inventory.quantity,
        reserved: inventory.reserved,
        low_stock_threshold: product_variants.low_stock_threshold,
      })
      .from(inventory)
      .innerJoin(
        product_variants,
        eq(product_variants.id, inventory.variant_id),
      )
      .innerJoin(products, eq(products.id, product_variants.product_id))
      .innerJoin(warehouses, eq(warehouses.id, inventory.warehouse_id))
      .where(
        and(
          eq(inventory.variant_id, input.variant_id),
          eq(inventory.warehouse_id, input.to_warehouse_id),
        ),
      )
      .limit(1);

    if (!updated) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao concluir transferência",
      });
    }

    return schema.admin.transferInventory.output.parse(
      mapInventoryRow(updated),
    );
  });
}

export async function exportInventoryCsv() {
  const { data } = await listInventory();
  const header =
    "sku,warehouse_code,product_name,size,color,quantity,reserved,available,low_stock_threshold";
  const lines = data.map((row) =>
    [
      row.sku,
      row.warehouse_code,
      `"${row.product_name.replaceAll('"', '""')}"`,
      row.size,
      `"${row.color.replaceAll('"', '""')}"`,
      row.quantity,
      row.reserved,
      row.available,
      row.low_stock_threshold,
    ].join(","),
  );

  return {
    filename: `estoque-lindaflor-${new Date().toISOString().slice(0, 10)}.csv`,
    content: [header, ...lines].join("\n"),
  };
}

function parseCsvLine(line: string) {
  const values: string[] = [];
  let current = "";
  let inQuotes = false;

  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    if (char === '"') {
      if (inQuotes && line[i + 1] === '"') {
        current += '"';
        i++;
      } else {
        inQuotes = !inQuotes;
      }
      continue;
    }
    if (char === "," && !inQuotes) {
      values.push(current.trim());
      current = "";
      continue;
    }
    current += char;
  }

  values.push(current.trim());
  return values;
}

export async function importInventoryCsv(
  input: ImportInventoryInput,
  createdBy: string | null,
) {
  const lines = input.csv.trim().split(/\r?\n/).filter(Boolean);
  if (lines.length < 2) {
    throw new ORPCError("BAD_REQUEST", {
      message: "CSV vazio ou sem linhas de dados",
    });
  }

  const headerLine = lines[0];
  if (!headerLine) {
    throw new ORPCError("BAD_REQUEST", {
      message: "CSV vazio ou sem linhas de dados",
    });
  }

  const header = parseCsvLine(headerLine).map((h) => h.toLowerCase());
  const skuIdx = header.indexOf("sku");
  const warehouseIdx = header.indexOf("warehouse_code");
  const quantityIdx = header.indexOf("quantity");

  if (skuIdx === -1 || quantityIdx === -1) {
    throw new ORPCError("BAD_REQUEST", {
      message: "CSV deve conter colunas sku e quantity",
    });
  }

  const [defaultWarehouseId, warehouseRows] = await Promise.all([
    getDefaultWarehouseId(),
    db.select({ id: warehouses.id, code: warehouses.code }).from(warehouses),
  ]);
  const warehouseByCode = new Map(
    warehouseRows.map((row) => [row.code.toLowerCase(), row.id]),
  );

  const results: Array<{
    sku: string;
    status: "updated" | "skipped" | "error";
    message?: string;
  }> = [];

  await Promise.all(
    lines.slice(1).map(async (line) => {
      const cols = parseCsvLine(line);
      const sku = cols[skuIdx]?.trim();
      const quantityRaw = cols[quantityIdx]?.trim();
      const warehouseCode =
        warehouseIdx >= 0
          ? cols[warehouseIdx]?.trim().toLowerCase()
          : "principal";

      if (!sku || !quantityRaw) {
        results.push({
          sku: sku ?? line,
          status: "skipped",
          message: "Linha inválida",
        });
        return;
      }

      const targetQuantity = Number(quantityRaw);
      if (!Number.isInteger(targetQuantity) || targetQuantity < 0) {
        results.push({ sku, status: "error", message: "Quantidade inválida" });
        return;
      }

      const warehouseId =
        warehouseByCode.get(warehouseCode ?? "principal") ?? defaultWarehouseId;

      try {
        const [variant] = await db
          .select({ id: product_variants.id })
          .from(product_variants)
          .where(eq(product_variants.sku, sku))
          .limit(1);

        if (!variant) {
          results.push({ sku, status: "error", message: "SKU não encontrado" });
          return;
        }

        const [current] = await db
          .select({ quantity: inventory.quantity })
          .from(inventory)
          .where(
            and(
              eq(inventory.variant_id, variant.id),
              eq(inventory.warehouse_id, warehouseId),
            ),
          )
          .limit(1);

        if (!current) {
          await db.insert(inventory).values({
            variant_id: variant.id,
            warehouse_id: warehouseId,
            quantity: targetQuantity,
            reserved: 0,
          });
          await db.insert(inventory_movements).values({
            variant_id: variant.id,
            warehouse_id: warehouseId,
            type: "entrada",
            quantity: targetQuantity,
            notes: "Importação CSV",
            created_by: createdBy,
          });
          results.push({ sku, status: "updated" });
          return;
        }

        const delta = targetQuantity - current.quantity;
        if (delta === 0) {
          results.push({ sku, status: "skipped", message: "Sem alteração" });
          return;
        }

        await applyInventoryChange({
          variant_id: variant.id,
          warehouse_id: warehouseId,
          quantity_delta: delta,
          type: delta > 0 ? "entrada" : "ajuste",
          notes: "Importação CSV",
          created_by: createdBy,
        });
        results.push({ sku, status: "updated" });
      } catch (error) {
        results.push({
          sku,
          status: "error",
          message: error instanceof Error ? error.message : "Erro desconhecido",
        });
      }
    }),
  );

  return schema.admin.importInventoryCsv.output.parse({
    processed: results.length,
    updated: results.filter((r) => r.status === "updated").length,
    skipped: results.filter((r) => r.status === "skipped").length,
    errors: results.filter((r) => r.status === "error").length,
    results,
  });
}

type InventoryExecutor = {
  update: typeof db.update;
  insert: typeof db.insert;
  select: typeof db.select;
  execute: typeof db.execute;
};

async function resolveWarehouseId(warehouseId?: string) {
  return warehouseId ?? (await getDefaultWarehouseId());
}

/**
 * Atomically reserve stock when available >= quantity.
 * Uses conditional update to prevent oversell under concurrency.
 */
export async function reserveFulfillmentStock(
  params: {
    variant_id: string;
    quantity: number;
    order_id: string;
    warehouse_id: string;
    notes?: string;
  },
  executor: InventoryExecutor = db,
) {
  const updated = await executor
    .update(inventory)
    .set({
      reserved: sql`${inventory.reserved} + ${params.quantity}`,
    })
    .where(
      and(
        eq(inventory.variant_id, params.variant_id),
        eq(inventory.warehouse_id, params.warehouse_id),
        sql`${inventory.quantity} - ${inventory.reserved} >= ${params.quantity}`,
      ),
    )
    .returning({ id: inventory.id });

  if (updated.length === 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Estoque insuficiente para reserva",
    });
  }

  await executor.insert(inventory_movements).values({
    variant_id: params.variant_id,
    warehouse_id: params.warehouse_id,
    type: "reserva",
    quantity: params.quantity,
    reference_type: "order",
    reference_id: params.order_id,
    notes: params.notes ?? "Reserva de checkout",
  });
}

export async function releaseFulfillmentStock(
  params: {
    variant_id: string;
    quantity: number;
    order_id: string;
    warehouse_id?: string | null;
    notes?: string;
  },
  executor: InventoryExecutor = db,
) {
  const warehouseId = await resolveWarehouseId(
    params.warehouse_id ?? undefined,
  );

  await executor
    .update(inventory)
    .set({
      reserved: sql`GREATEST(${inventory.reserved} - ${params.quantity}, 0)`,
    })
    .where(
      and(
        eq(inventory.variant_id, params.variant_id),
        eq(inventory.warehouse_id, warehouseId),
      ),
    );

  await executor.insert(inventory_movements).values({
    variant_id: params.variant_id,
    warehouse_id: warehouseId,
    type: "liberacao",
    quantity: params.quantity,
    reference_type: "order",
    reference_id: params.order_id,
    notes: params.notes ?? "Reserva liberada",
  });
}

export async function confirmFulfillmentSale(
  params: {
    variant_id: string;
    quantity: number;
    order_id: string;
    warehouse_id?: string | null;
  },
  executor: InventoryExecutor = db,
) {
  const warehouseId = await resolveWarehouseId(
    params.warehouse_id ?? undefined,
  );

  await executor
    .update(inventory)
    .set({
      quantity: sql`${inventory.quantity} - ${params.quantity}`,
      reserved: sql`GREATEST(${inventory.reserved} - ${params.quantity}, 0)`,
    })
    .where(
      and(
        eq(inventory.variant_id, params.variant_id),
        eq(inventory.warehouse_id, warehouseId),
      ),
    );

  await executor.insert(inventory_movements).values({
    variant_id: params.variant_id,
    warehouse_id: warehouseId,
    type: "venda",
    quantity: params.quantity,
    reference_type: "order",
    reference_id: params.order_id,
    notes: "Pagamento confirmado",
  });
}

export async function getAvailableTotal(variantId: string) {
  const [row] = await db
    .select({
      available: sql<number>`coalesce(sum(greatest(${inventory.quantity} - ${inventory.reserved}, 0)), 0)::int`,
    })
    .from(inventory)
    .innerJoin(warehouses, eq(warehouses.id, inventory.warehouse_id))
    .where(
      and(eq(inventory.variant_id, variantId), eq(warehouses.active, true)),
    );

  return row?.available ?? 0;
}

/** @deprecated Use getAvailableTotal — aggregates across active warehouses. */
export async function getFulfillmentAvailable(variantId: string) {
  return getAvailableTotal(variantId);
}

export async function issueInventory(
  input: z.infer<typeof schema.admin.issueInventory.input>,
  createdBy: string | null,
) {
  if (input.quantity <= 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Quantidade de saída deve ser positiva",
    });
  }

  const warehouseId = input.warehouse_id ?? (await getDefaultWarehouseId());

  const result = await applyInventoryChange({
    variant_id: input.variant_id,
    warehouse_id: warehouseId,
    quantity_delta: -input.quantity,
    type: "saida",
    notes: input.notes ?? "Saída de estoque",
    created_by: createdBy,
  });

  return schema.admin.issueInventory.output.parse(result);
}

export async function updateVariantLowStockThreshold(
  input: z.infer<typeof schema.admin.updateVariantLowStockThreshold.input>,
) {
  const [updated] = await db
    .update(product_variants)
    .set({ low_stock_threshold: input.low_stock_threshold })
    .where(eq(product_variants.id, input.variant_id))
    .returning({
      variant_id: product_variants.id,
      low_stock_threshold: product_variants.low_stock_threshold,
    });

  if (!updated) {
    throw new ORPCError("NOT_FOUND", { message: "Variante não encontrada" });
  }

  return schema.admin.updateVariantLowStockThreshold.output.parse(updated);
}
