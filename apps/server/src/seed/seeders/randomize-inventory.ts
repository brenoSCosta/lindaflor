import { db } from "@lindaflor/db";
import { inventory } from "@lindaflor/db/schema/commerce";
import { eq } from "drizzle-orm";

/** Estoque aleatório: 0–3 esgotado, 4–25 normal, ~15% com estoque alto */
export function randomStockQuantity(): number {
  const roll = Math.random();

  if (roll < 0.12) {
    return 0;
  }
  if (roll < 0.85) {
    return Math.floor(Math.random() * 22) + 4;
  }
  return Math.floor(Math.random() * 35) + 26;
}

export async function randomizeInventory() {
  const rows = await db.select().from(inventory);

  for (const row of rows) {
    const quantity = randomStockQuantity();
    const reserved =
      quantity === 0
        ? 0
        : Math.min(
            row.reserved,
            Math.floor(Math.random() * Math.min(quantity, 3)),
          );

    await db
      .update(inventory)
      .set({ quantity, reserved })
      .where(eq(inventory.variant_id, row.variant_id));
  }

  return rows.length;
}
