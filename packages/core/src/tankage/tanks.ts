import { loadTankSnapshots } from "@lindaflor/core/tankage/snapshot";
import { db } from "@lindaflor/db";
import {
  concessions,
  installations,
  measurement_equipments,
  tankages,
  tanks,
} from "@lindaflor/db/schema/tankage";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import { operationalDayKey } from "@lindaflor/shared/lib/zoned-datetime";
import { schema } from "@lindaflor/shared/schemas/tankage";
import { ORPCError } from "@orpc/server";
import {
  and,
  asc,
  count,
  desc,
  eq,
  ilike,
  inArray,
  like,
  or,
  type SQL,
} from "drizzle-orm";
import type { z } from "zod";

export const tankOutputColumns = {
  id: tanks.id,
  tag: tanks.tag,
  concession_id: tanks.concession_id,
  concession_name: concessions.name,
  installation_id: tanks.installation_id,
  installation_name: installations.name,
  measurement_equipment_id: tanks.measurement_equipment_id,
  measurement_equipment_code: measurement_equipments.code,
  latitude: tanks.latitude,
  longitude: tanks.longitude,
  organization_id: tanks.organization_id,
  created_by_user_id: tanks.created_by_user_id,
  created_at: tanks.created_at,
  updated_at: tanks.updated_at,
};

async function selectTankById(id: string) {
  const [row] = await db
    .select(tankOutputColumns)
    .from(tanks)
    .innerJoin(concessions, eq(tanks.concession_id, concessions.id))
    .innerJoin(installations, eq(tanks.installation_id, installations.id))
    .leftJoin(
      measurement_equipments,
      eq(tanks.measurement_equipment_id, measurement_equipments.id),
    )
    .where(eq(tanks.id, id));
  return row;
}

async function assertMeasurementEquipmentInOrg(
  equipmentId: string | null | undefined,
  organizationId: string,
): Promise<void> {
  if (equipmentId == null) return;
  const [equipment] = await db
    .select({ id: measurement_equipments.id })
    .from(measurement_equipments)
    .where(
      and(
        eq(measurement_equipments.id, equipmentId),
        eq(measurement_equipments.organization_id, organizationId),
      ),
    );
  if (!equipment) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Trena inválida",
    });
  }
}

export async function listAllTanks(args: {
  input: z.infer<typeof schema.v1.tank.list.all.input> | null | undefined;
  organizationId: string;
}) {
  const baseFilter = eq(tanks.organization_id, args.organizationId);

  if (!args.input) {
    const data = await db
      .select(tankOutputColumns)
      .from(tanks)
      .innerJoin(concessions, eq(tanks.concession_id, concessions.id))
      .innerJoin(installations, eq(tanks.installation_id, installations.id))
      .leftJoin(
        measurement_equipments,
        eq(tanks.measurement_equipment_id, measurement_equipments.id),
      )
      .where(baseFilter)
      .orderBy(asc(tanks.tag));
    return {
      data: schema.v1.tank.list.all.output.shape.data.parse(data),
    };
  }

  const { pageIndex, pageSize } = args.input.pagination ?? {
    pageIndex: 0,
    pageSize: 10,
  };
  const sorting = args.input.sorting ?? [];
  const columnFilters = args.input.columnFilters ?? [];
  const globalFilter = args.input.globalFilter;

  const globalFilterCondition = globalFilter
    ? or(
        ilike(tanks.tag, `%${globalFilter}%`),
        ilike(concessions.name, `%${globalFilter}%`),
        ilike(installations.name, `%${globalFilter}%`),
      )
    : undefined;

  const dynamicFilters = columnFilters.flatMap((f) => {
    const condition: SQL | undefined = (() => {
      switch (f.id) {
        case "tag":
          if (typeof f.value !== "string") return undefined;
          return like(tanks.tag, `%${f.value}%`);
        case "concession_id":
          if (typeof f.value !== "string") return undefined;
          return eq(tanks.concession_id, f.value);
        case "installation_id":
          if (typeof f.value !== "string") return undefined;
          return eq(tanks.installation_id, f.value);
        default:
          return undefined;
      }
    })();
    return condition ? [condition] : [];
  });

  const whereClause = and(baseFilter, globalFilterCondition, ...dynamicFilters);

  const orderByClause = sorting
    .map((s) => {
      switch (s.id) {
        case "tag":
          return s.desc ? desc(tanks.tag) : asc(tanks.tag);
        case "created_at":
          return s.desc ? desc(tanks.created_at) : asc(tanks.created_at);
        case "updated_at":
          return s.desc ? desc(tanks.updated_at) : asc(tanks.updated_at);
        default:
          return undefined;
      }
    })
    .filter((c): c is NonNullable<typeof c> => c !== undefined);

  const [[rowCountResult], data] = await Promise.all([
    db
      .select({ count: count() })
      .from(tanks)
      .innerJoin(concessions, eq(tanks.concession_id, concessions.id))
      .innerJoin(installations, eq(tanks.installation_id, installations.id))
      .leftJoin(
        measurement_equipments,
        eq(tanks.measurement_equipment_id, measurement_equipments.id),
      )
      .where(whereClause),
    db
      .select(tankOutputColumns)
      .from(tanks)
      .innerJoin(concessions, eq(tanks.concession_id, concessions.id))
      .innerJoin(installations, eq(tanks.installation_id, installations.id))
      .leftJoin(
        measurement_equipments,
        eq(tanks.measurement_equipment_id, measurement_equipments.id),
      )
      .where(whereClause)
      .orderBy(...(orderByClause.length > 0 ? orderByClause : [asc(tanks.tag)]))
      .limit(pageSize)
      .offset(pageIndex * pageSize),
  ]);

  return schema.v1.tank.list.all.output.parse({
    data,
    meta: {
      rowCount: rowCountResult?.count ?? 0,
    },
  });
}

export async function listTankSnapshots(args: {
  input: z.infer<typeof schema.v1.tank.list.snapshot.input> | null | undefined;
  organizationId: string;
  timezone: string;
}) {
  const productionOperationalDay = operationalDayKey(new Date(), args.timezone);
  const snapshots = await loadTankSnapshots({
    organizationId: args.organizationId,
    tankIds: args.input?.ids,
    at: args.input?.at ?? operationalDayKey(new Date(), args.timezone),
    productionOperationalDay,
  });
  return schema.v1.tank.list.snapshot.output.parse({
    data: snapshots,
  });
}

export async function getTankById(args: {
  input: { id: string };
  ability: AppAbility;
}) {
  const row = await selectTankById(args.input.id);
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tanque não encontrado",
    });
  }
  if (args.ability.cannot("read", subject("Tanks", row))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler este tanque",
    });
  }
  return schema.v1.tank.getBy.id.output.parse(row);
}

export async function getTankByTag(args: {
  input: { tag: string };
  organizationId: string;
  ability: AppAbility;
}) {
  const [row] = await db
    .select(tankOutputColumns)
    .from(tanks)
    .innerJoin(concessions, eq(tanks.concession_id, concessions.id))
    .innerJoin(installations, eq(tanks.installation_id, installations.id))
    .leftJoin(
      measurement_equipments,
      eq(tanks.measurement_equipment_id, measurement_equipments.id),
    )
    .where(
      and(
        eq(tanks.organization_id, args.organizationId),
        eq(tanks.tag, args.input.tag),
      ),
    );
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tanque não cadastrado",
    });
  }
  if (args.ability.cannot("read", subject("Tanks", row))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler este tanque",
    });
  }
  return schema.v1.tank.getBy.tag.output.parse(row);
}

export async function createTank(args: {
  input: z.infer<typeof schema.v1.tank.create.input>;
  organizationId: string;
  createdByUserId: string;
}) {
  const [installation] = await db
    .select()
    .from(installations)
    .where(
      and(
        eq(installations.id, args.input.installation_id),
        eq(installations.organization_id, args.organizationId),
      ),
    );
  if (!installation) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Instalação inválida",
    });
  }
  if (installation.concession_id !== args.input.concession_id) {
    throw new ORPCError("BAD_REQUEST", {
      message: "A instalação não pertence à concessão selecionada",
    });
  }

  const [duplicate] = await db
    .select({ id: tanks.id })
    .from(tanks)
    .where(
      and(
        eq(tanks.organization_id, args.organizationId),
        eq(tanks.tag, args.input.tag),
      ),
    );
  if (duplicate) {
    throw new ORPCError("CONFLICT", {
      message: "TAG já cadastrada",
    });
  }

  await assertMeasurementEquipmentInOrg(
    args.input.measurement_equipment_id,
    args.organizationId,
  );

  const [created] = await db
    .insert(tanks)
    .values({
      tag: args.input.tag,
      concession_id: args.input.concession_id,
      installation_id: args.input.installation_id,
      measurement_equipment_id: args.input.measurement_equipment_id ?? null,
      latitude: args.input.latitude ?? null,
      longitude: args.input.longitude ?? null,
      organization_id: args.organizationId,
      created_by_user_id: args.createdByUserId,
    })
    .returning({ id: tanks.id });
  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar tanque",
    });
  }
  const row = await selectTankById(created.id);
  if (!row) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao carregar tanque criado",
    });
  }
  return schema.v1.tank.create.output.parse(row);
}

export async function updateTank(args: {
  input: z.infer<typeof schema.v1.tank.update.input>;
  ability: AppAbility;
}) {
  const { id, ...data } = args.input;
  const existing = await selectTankById(id);
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tanque não encontrado",
    });
  }
  if (args.ability.cannot("update", subject("Tanks", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para atualizar este tanque",
    });
  }

  const nextConcessionId = data.concession_id ?? existing.concession_id;
  const nextInstallationId = data.installation_id ?? existing.installation_id;
  if (data.concession_id != null || data.installation_id != null) {
    const [installation] = await db
      .select()
      .from(installations)
      .where(
        and(
          eq(installations.id, nextInstallationId),
          eq(installations.organization_id, existing.organization_id),
        ),
      );
    if (!installation || installation.concession_id !== nextConcessionId) {
      throw new ORPCError("BAD_REQUEST", {
        message: "A instalação não pertence à concessão selecionada",
      });
    }
  }

  if (data.tag != null && data.tag !== existing.tag) {
    const [duplicate] = await db
      .select({ id: tanks.id })
      .from(tanks)
      .where(
        and(
          eq(tanks.organization_id, existing.organization_id),
          eq(tanks.tag, data.tag),
        ),
      );
    if (duplicate) {
      throw new ORPCError("CONFLICT", {
        message: "TAG já cadastrada",
      });
    }
  }

  if (data.measurement_equipment_id !== undefined) {
    await assertMeasurementEquipmentInOrg(
      data.measurement_equipment_id,
      existing.organization_id,
    );
  }

  await db.update(tanks).set(data).where(eq(tanks.id, id));
  const updated = await selectTankById(id);
  if (!updated) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tanque não encontrado",
    });
  }
  return schema.v1.tank.update.output.parse(updated);
}

export async function deleteTanks(args: {
  input: { ids: string[] };
  ability: AppAbility;
}) {
  const rows = await db
    .select(tankOutputColumns)
    .from(tanks)
    .innerJoin(concessions, eq(tanks.concession_id, concessions.id))
    .innerJoin(installations, eq(tanks.installation_id, installations.id))
    .leftJoin(
      measurement_equipments,
      eq(tanks.measurement_equipment_id, measurement_equipments.id),
    )
    .where(inArray(tanks.id, args.input.ids));
  if (rows.length === 0) {
    throw new ORPCError("NOT_FOUND", {
      message: "Nenhum tanque encontrado",
    });
  }
  const notFoundIds = args.input.ids.filter(
    (id) => !rows.some((r) => r.id === id),
  );
  if (notFoundIds.length > 0) {
    throw new ORPCError("NOT_FOUND", {
      message: `Tanques não encontrados: ${notFoundIds.join(", ")}`,
    });
  }
  const unauthorized = rows.filter((r) =>
    args.ability.cannot("delete", subject("Tanks", r)),
  );
  if (unauthorized.length > 0) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para excluir um ou mais destes tanques",
    });
  }

  const [referenced] = await db
    .select({ count: count() })
    .from(tankages)
    .where(inArray(tankages.tank_id, args.input.ids));
  if ((referenced?.count ?? 0) > 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Tanque possui medições e não pode ser excluído",
    });
  }

  await db.delete(tanks).where(inArray(tanks.id, args.input.ids));
  return null;
}

export async function getTankSnapshot(args: {
  input: { id: string; at?: string };
  organizationId: string;
  timezone: string;
}) {
  const at = args.input.at ?? operationalDayKey(new Date(), args.timezone);
  const snapshots = await loadTankSnapshots({
    organizationId: args.organizationId,
    tankIds: [args.input.id],
    at,
  });
  const snapshot = snapshots[0];
  if (!snapshot) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tanque não encontrado",
    });
  }
  return schema.v1.tank.get.snapshot.output.parse(snapshot);
}
