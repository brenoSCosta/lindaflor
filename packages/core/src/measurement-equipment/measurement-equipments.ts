import { db } from "@lindaflor/db";
import { measurement_equipments } from "@lindaflor/db/schema/tankage";
import {
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/measurement-equipment";
import { ORPCError } from "@orpc/server";
import { and, asc, eq, ilike, inArray, or } from "drizzle-orm";
import type { z } from "zod";

type GetAllMeasurementEquipmentsInput = z.infer<typeof schema.v1.getAll.input>;

export async function getAllMeasurementEquipments(params: {
  input: GetAllMeasurementEquipmentsInput | undefined;
  organizationId: string;
}) {
  const { input, organizationId } = params;
  const baseFilter = eq(measurement_equipments.organization_id, organizationId);

  if (!input) {
    const data = await db
      .select()
      .from(measurement_equipments)
      .where(baseFilter)
      .orderBy(asc(measurement_equipments.code));
    return { data: schema.v1.getAll.output.shape.data.parse(data) };
  }

  const { pageIndex, pageSize } = input.pagination ?? {
    pageIndex: 0,
    pageSize: 10,
  };
  const globalFilter = input.globalFilter;

  const globalFilterCondition = globalFilter
    ? or(
        ilike(measurement_equipments.code, `%${globalFilter}%`),
        ilike(measurement_equipments.description, `%${globalFilter}%`),
        ilike(measurement_equipments.manufacturer, `%${globalFilter}%`),
        ilike(measurement_equipments.serial_number, `%${globalFilter}%`),
      )
    : undefined;

  const whereClause = and(baseFilter, globalFilterCondition);

  const data = await db
    .select()
    .from(measurement_equipments)
    .where(whereClause)
    .limit(pageSize)
    .offset(pageIndex * pageSize);

  return schema.v1.getAll.output.parse({
    data,
  });
}

type GetMeasurementEquipmentByIdInput = z.infer<typeof schema.v1.getById.input>;

export async function getMeasurementEquipmentById(params: {
  input: GetMeasurementEquipmentByIdInput;
  ability: AppAbility;
}) {
  const [row] = await db
    .select()
    .from(measurement_equipments)
    .where(eq(measurement_equipments.id, params.input.id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Trena não encontrada",
    });
  }
  if (params.ability.cannot("read", subject("MeasurementEquipments", row))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler esta trena",
    });
  }
  return schema.v1.getById.output.parse(row);
}

type CreateMeasurementEquipmentInput = z.infer<typeof schema.v1.create.input>;

export async function createMeasurementEquipment(params: {
  input: CreateMeasurementEquipmentInput;
  organizationId: string;
  userId: string;
}) {
  const { input, organizationId, userId } = params;

  const [duplicate] = await db
    .select({ id: measurement_equipments.id })
    .from(measurement_equipments)
    .where(
      and(
        eq(measurement_equipments.organization_id, organizationId),
        eq(measurement_equipments.code, input.code),
      ),
    );
  if (duplicate) {
    throw new ORPCError("CONFLICT", {
      message: "Código de trena já cadastrado",
    });
  }

  const [created] = await db
    .insert(measurement_equipments)
    .values({
      code: input.code,
      description: input.description ?? null,
      type: input.type ?? "manual",
      length_m: input.length_m ?? null,
      reference_height_m: input.reference_height_m ?? null,
      manufacturer: input.manufacturer ?? null,
      serial_number: input.serial_number ?? null,
      calibrated_at: input.calibrated_at ?? null,
      calibration_valid_until: input.calibration_valid_until ?? null,
      active: input.active ?? true,
      organization_id: organizationId,
      created_by_user_id: userId,
    })
    .returning();
  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar trena",
    });
  }
  return schema.v1.create.output.parse(created);
}

type UpdateMeasurementEquipmentInput = z.infer<typeof schema.v1.update.input>;

export async function updateMeasurementEquipment(params: {
  input: UpdateMeasurementEquipmentInput;
  ability: AppAbility;
}) {
  const { id, ...data } = params.input;
  const [existing] = await db
    .select()
    .from(measurement_equipments)
    .where(eq(measurement_equipments.id, id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Trena não encontrada",
    });
  }
  if (
    params.ability.cannot("update", subject("MeasurementEquipments", existing))
  ) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para atualizar esta trena",
    });
  }

  if (data.code != null && data.code !== existing.code) {
    const [duplicate] = await db
      .select({ id: measurement_equipments.id })
      .from(measurement_equipments)
      .where(
        and(
          eq(measurement_equipments.organization_id, existing.organization_id),
          eq(measurement_equipments.code, data.code),
        ),
      );
    if (duplicate) {
      throw new ORPCError("CONFLICT", {
        message: "Código de trena já cadastrado",
      });
    }
  }

  await db
    .update(measurement_equipments)
    .set(data)
    .where(eq(measurement_equipments.id, id));
  const [updated] = await db
    .select()
    .from(measurement_equipments)
    .where(eq(measurement_equipments.id, id));
  if (!updated) {
    throw new ORPCError("NOT_FOUND", {
      message: "Trena não encontrada",
    });
  }
  return schema.v1.update.output.parse(updated);
}

type DeleteMeasurementEquipmentsInput = z.infer<typeof schema.v1.delete.input>;

export async function deleteMeasurementEquipments(params: {
  input: DeleteMeasurementEquipmentsInput;
  ability: AppAbility;
}) {
  const rows = await db
    .select()
    .from(measurement_equipments)
    .where(inArray(measurement_equipments.id, params.input.ids));
  if (rows.length === 0) {
    throw new ORPCError("NOT_FOUND", {
      message: "Nenhuma trena encontrada",
    });
  }
  const notFoundIds = params.input.ids.filter(
    (id) => !rows.some((r) => r.id === id),
  );
  if (notFoundIds.length > 0) {
    throw new ORPCError("NOT_FOUND", {
      message: `Trenas não encontradas: ${notFoundIds.join(", ")}`,
    });
  }
  const unauthorized = rows.filter((r) =>
    params.ability.cannot("delete", subject("MeasurementEquipments", r)),
  );
  if (unauthorized.length > 0) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para excluir uma ou mais destas trenas",
    });
  }
  await db
    .delete(measurement_equipments)
    .where(inArray(measurement_equipments.id, params.input.ids));
  return null;
}
