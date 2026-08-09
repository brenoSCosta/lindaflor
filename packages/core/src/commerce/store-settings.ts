import { db } from "@lindaflor/db";
import { store_settings } from "@lindaflor/db/schema/commerce";
import { schema } from "@lindaflor/shared/schemas/commerce";
import { ORPCError } from "@orpc/server";
import { asc, eq } from "drizzle-orm";
import type { z } from "zod";

type UpdateStoreSettingsInput = z.infer<
  typeof schema.admin.updateStoreSettings.input
>;

function mapStoreSettings(row: typeof store_settings.$inferSelect) {
  return schema.admin.getStoreSettings.output.parse({
    id: row.id,
    pix_key: row.pix_key,
    pix_key_type: row.pix_key_type,
    pix_merchant_name: row.pix_merchant_name,
    pix_merchant_city: row.pix_merchant_city,
    whatsapp_number: row.whatsapp_number,
    whatsapp_message_template: row.whatsapp_message_template,
    updated_at: row.updated_at,
  });
}

export async function ensureStoreSettings() {
  const [existing] = await db
    .select()
    .from(store_settings)
    .orderBy(asc(store_settings.created_at))
    .limit(1);

  if (existing) {
    return existing;
  }

  const [created] = await db
    .insert(store_settings)
    .values({
      whatsapp_message_template:
        "Olá! Fiz o pedido {{order_id}} no valor de {{total}} e quero confirmar o pagamento via PIX.",
    })
    .returning();

  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar configurações da loja",
    });
  }

  return created;
}

export async function getStoreSettings() {
  const row = await ensureStoreSettings();
  return mapStoreSettings(row);
}

export function isStaticPixConfigured(settings: {
  pix_key: string | null;
  pix_key_type: string | null;
  pix_merchant_name: string | null;
  pix_merchant_city: string | null;
}) {
  return Boolean(
    settings.pix_key?.trim() &&
    settings.pix_key_type &&
    settings.pix_merchant_name?.trim() &&
    settings.pix_merchant_city?.trim(),
  );
}

export async function updateStoreSettings(input: UpdateStoreSettingsInput) {
  const existing = await ensureStoreSettings();

  const [updated] = await db
    .update(store_settings)
    .set({
      pix_key: input.pix_key?.trim() || null,
      pix_key_type: input.pix_key_type,
      pix_merchant_name: input.pix_merchant_name?.trim() || null,
      pix_merchant_city: input.pix_merchant_city?.trim() || null,
      whatsapp_number: input.whatsapp_number?.trim() || null,
      whatsapp_message_template:
        input.whatsapp_message_template?.trim() || null,
    })
    .where(eq(store_settings.id, existing.id))
    .returning();

  if (!updated) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao atualizar configurações da loja",
    });
  }

  return mapStoreSettings(updated);
}
