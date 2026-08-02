import {
  recordAuditEvents,
  tankDayAggregateId,
} from "@lindaflor/core/lib/audit/record";
import { dateFilterToCondition } from "@lindaflor/core/lib/date-filter";
import { getTankDayBulletinStatus } from "@lindaflor/core/tankage/day-bulletin";
import { assertTankDayMeasurementCapacity } from "@lindaflor/core/tankage/day-measurement-limit";
import {
  assertTankageMeasurementWithinBounds,
  loadTankageMeasurementContext,
} from "@lindaflor/core/tankage/tankage-measurement-bounds";
import { assertTankageMeasurementStock } from "@lindaflor/core/tankage/tankage-stock-validation";
import { computeTankageVolumeColumns } from "@lindaflor/core/tankage/tankage-volume-persist";
import { db } from "@lindaflor/db";
import { users } from "@lindaflor/db/schema/auth";
import {
  concessions,
  installations,
  measurement_equipments,
  tank_day_bulletins,
  tank_transfers,
  tankages,
  tanks,
} from "@lindaflor/db/schema/tankage";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import { buildTankageAuditChanges } from "@lindaflor/shared/lib/audit/tankage-changes";
import { parseDateFilterValue } from "@lindaflor/shared/lib/date-filter";
import type {
  ColumnFilters,
  FacetsSchema,
  GlobalFilter,
  RowSelection,
  Sorting,
} from "@lindaflor/shared/lib/utils";
import { operationalDayKey } from "@lindaflor/shared/lib/zoned-datetime";
import { schema } from "@lindaflor/shared/schemas/tankage";
import { defaultFacets } from "@lindaflor/shared/schemas/tankage/tankages";
import { ORPCError } from "@orpc/server";
import {
  and,
  asc,
  desc,
  eq,
  ilike,
  inArray,
  like,
  not,
  or,
  sql,
  type SQL,
} from "drizzle-orm";
import type { z } from "zod";

const tankageOutputColumns = {
  id: tankages.id,
  tank_id: tankages.tank_id,
  tag: tanks.tag,
  concession_id: tankages.concession_id,
  installation_id: tankages.installation_id,
  measurement_equipment_id: tankages.measurement_equipment_id,
  measurement_equipment_code: measurement_equipments.code,
  operator_user_id: tankages.operator_user_id,
  previous_measurement: tankages.previous_measurement,
  current_measurement: tankages.current_measurement,
  measured_at: tankages.measured_at,
  operational_day: tankages.operational_day,
  oil_temperature_c: tankages.oil_temperature_c,
  ambient_temperature_c: tankages.ambient_temperature_c,
  observation: tankages.observation,
  latitude: tankages.latitude,
  longitude: tankages.longitude,
  gross_volume_m3: tankages.gross_volume_m3,
  gross_volume_m3_20c: tankages.gross_volume_m3_20c,
  net_oil_volume_m3_20c: tankages.net_oil_volume_m3_20c,
  volume_oil_barrels: tankages.volume_oil_barrels,
  shell_temperature_c: tankages.shell_temperature_c,
  shell_correction_factor: tankages.shell_correction_factor,
  liquid_correction_factor: tankages.liquid_correction_factor,
  combined_correction_factor: tankages.combined_correction_factor,
  tank_calibration_id: tankages.tank_calibration_id,
  lab_oil_analysis_id: tankages.lab_oil_analysis_id,
  density_at_20c_kg_m3: tankages.density_at_20c_kg_m3,
  water_and_sediment_percent: tankages.water_and_sediment_percent,
  organization_id: tankages.organization_id,
  created_by_user_id: tankages.created_by_user_id,
  created_at: tankages.created_at,
  updated_at: tankages.updated_at,
  concession_name: concessions.name,
  installation_name: installations.name,
  operator_name: users.name,
  bulletin_status: sql<
    "open" | "approved"
  >`coalesce(${tank_day_bulletins.status}, 'open')`.as("bulletin_status"),
};

const tankageBulletinJoin = and(
  eq(tank_day_bulletins.organization_id, tankages.organization_id),
  eq(tank_day_bulletins.tank_id, tankages.tank_id),
  eq(tank_day_bulletins.operational_day, tankages.operational_day),
);

export async function listAllTankages(args: {
  input: z.infer<typeof schema.v1.tankage.list.all.input> | null | undefined;
  organizationId: string;
  timezone: string;
}) {
  const baseFilter = eq(tankages.organization_id, args.organizationId);

  if (!args.input) {
    const rows = await db
      .select(tankageOutputColumns)
      .from(tankages)
      .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
      .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
      .innerJoin(installations, eq(tankages.installation_id, installations.id))
      .innerJoin(users, eq(tankages.operator_user_id, users.id))
      .leftJoin(
        measurement_equipments,
        eq(tankages.measurement_equipment_id, measurement_equipments.id),
      )
      .leftJoin(tank_day_bulletins, tankageBulletinJoin)
      .where(baseFilter)
      .orderBy(asc(tankages.id));
    return {
      data: schema.v1.tankage.list.all.output.shape.data.parse(rows),
    };
  }

  const { pageIndex, pageSize } = args.input.pagination ?? {
    pageIndex: 0,
    pageSize: 10,
  };
  const sorting = args.input.sorting ?? [];
  const columnFilters = args.input.columnFilters ?? [];
  const globalFilter = args.input.globalFilter;
  const pinnedTop = args.input.rowPinning?.top ?? [];
  const pinnedBottom = args.input.rowPinning?.bottom ?? [];
  const keepPinning = args.input.keepPinnedRows ?? true;
  const hasPinning = pinnedTop.length + pinnedBottom.length > 0;
  const clientTimezone = args.timezone;

  const whereClause = buildWhereClause({
    organizationId: args.organizationId,
    columnFilters,
    globalFilter,
    clientTimezone,
  });

  const effectiveOrderBy = buildOrderBy(sorting);

  const [mainRows, rowCount, concessionRows, installationRows, operatorRows] =
    await Promise.all([
      db
        .select(tankageOutputColumns)
        .from(tankages)
        .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
        .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
        .innerJoin(
          installations,
          eq(tankages.installation_id, installations.id),
        )
        .innerJoin(users, eq(tankages.operator_user_id, users.id))
        .leftJoin(
          measurement_equipments,
          eq(tankages.measurement_equipment_id, measurement_equipments.id),
        )
        .leftJoin(tank_day_bulletins, tankageBulletinJoin)
        .where(whereClause)
        .orderBy(...effectiveOrderBy)
        .limit(pageSize)
        .offset(pageIndex * pageSize),
      db
        .select({ count: sql<number>`count(*)::int` })
        .from(tankages)
        .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
        .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
        .innerJoin(
          installations,
          eq(tankages.installation_id, installations.id),
        )
        .innerJoin(users, eq(tankages.operator_user_id, users.id))
        .leftJoin(
          measurement_equipments,
          eq(tankages.measurement_equipment_id, measurement_equipments.id),
        )
        .leftJoin(tank_day_bulletins, tankageBulletinJoin)
        .where(whereClause)
        .then((rows) => rows[0]?.count ?? 0),
      db
        .select({
          id: concessions.id,
          name: concessions.name,
          count: sql<number>`count(*)::int`,
        })
        .from(tankages)
        .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
        .where(baseFilter)
        .groupBy(concessions.id, concessions.name),
      db
        .select({
          id: installations.id,
          name: installations.name,
          count: sql<number>`count(*)::int`,
        })
        .from(tankages)
        .innerJoin(
          installations,
          eq(tankages.installation_id, installations.id),
        )
        .where(baseFilter)
        .groupBy(installations.id, installations.name),
      db
        .select({
          id: users.id,
          name: users.name,
          count: sql<number>`count(*)::int`,
        })
        .from(tankages)
        .innerJoin(users, eq(tankages.operator_user_id, users.id))
        .leftJoin(
          measurement_equipments,
          eq(tankages.measurement_equipment_id, measurement_equipments.id),
        )
        .leftJoin(tank_day_bulletins, tankageBulletinJoin)
        .where(baseFilter)
        .groupBy(users.id, users.name),
    ]);

  const data =
    hasPinning && keepPinning
      ? await (async () => {
          const fetchedIds = new Set(mainRows.map((r) => r.id));
          const missingTop = pinnedTop.filter((id) => !fetchedIds.has(id));
          const missingBottom = pinnedBottom.filter(
            (id) => !fetchedIds.has(id),
          );

          const [pinnedTopRows, pinnedBottomRows] = await Promise.all([
            missingTop.length > 0
              ? db
                  .select(tankageOutputColumns)
                  .from(tankages)
                  .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
                  .innerJoin(
                    concessions,
                    eq(tankages.concession_id, concessions.id),
                  )
                  .innerJoin(
                    installations,
                    eq(tankages.installation_id, installations.id),
                  )
                  .innerJoin(users, eq(tankages.operator_user_id, users.id))
                  .leftJoin(
                    measurement_equipments,
                    eq(
                      tankages.measurement_equipment_id,
                      measurement_equipments.id,
                    ),
                  )
                  .leftJoin(tank_day_bulletins, tankageBulletinJoin)
                  .where(and(baseFilter, inArray(tankages.id, missingTop)))
              : Promise.resolve([]),
            missingBottom.length > 0
              ? db
                  .select(tankageOutputColumns)
                  .from(tankages)
                  .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
                  .innerJoin(
                    concessions,
                    eq(tankages.concession_id, concessions.id),
                  )
                  .innerJoin(
                    installations,
                    eq(tankages.installation_id, installations.id),
                  )
                  .innerJoin(users, eq(tankages.operator_user_id, users.id))
                  .leftJoin(
                    measurement_equipments,
                    eq(
                      tankages.measurement_equipment_id,
                      measurement_equipments.id,
                    ),
                  )
                  .leftJoin(tank_day_bulletins, tankageBulletinJoin)
                  .where(and(baseFilter, inArray(tankages.id, missingBottom)))
              : Promise.resolve([]),
          ]);

          return [...pinnedTopRows, ...mainRows, ...pinnedBottomRows];
        })()
      : mainRows;

  const facets: FacetsSchema = {
    concession_id: {
      type: "select",
      label: "Concessão",
      options: concessionRows.map((r) => ({
        value: r.id,
        label: r.name,
        count: r.count,
      })),
    },
    installation_id: {
      type: "select",
      label: "Instalação",
      options: installationRows.map((r) => ({
        value: r.id,
        label: r.name,
        count: r.count,
      })),
    },
    operator_user_id: {
      type: "select",
      label: "Operador",
      options: operatorRows.map((r) => ({
        value: r.id,
        label: r.name,
        count: r.count,
      })),
    },
    measured_at: defaultFacets.measured_at ?? {
      type: "date",
      label: "Data da medição",
    },
    created_at: defaultFacets.created_at ?? {
      type: "date",
      label: "Criado",
    },
    updated_at: defaultFacets.updated_at ?? {
      type: "date",
      label: "Atualizado",
    },
  };

  return schema.v1.tankage.list.all.output.parse({
    data,
    meta: { rowCount, facets },
  });
}

export async function getTankageById(args: {
  input: { id: string };
  ability: AppAbility;
}) {
  const [row] = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(eq(tankages.id, args.input.id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tancagem não encontrada",
    });
  }
  const output = schema.v1.tankage.getBy.id.output.parse(row);
  if (args.ability.cannot("read", subject("Tankages", output))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler esta tancagem",
    });
  }
  return output;
}

export async function listTankagesByTank(args: {
  input: z.infer<typeof schema.v1.tankage.listBy.tank.input>;
  organizationId: string;
}) {
  const [tank] = await db
    .select()
    .from(tanks)
    .where(
      and(
        eq(tanks.id, args.input.tank_id),
        eq(tanks.organization_id, args.organizationId),
      ),
    );
  if (!tank) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tanque não encontrado",
    });
  }

  let measuredOnCondition: ReturnType<typeof eq> | undefined;
  if (args.input.measured_on != null) {
    measuredOnCondition = eq(tankages.operational_day, args.input.measured_on);
  }

  const rows = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(
      and(
        eq(tankages.tank_id, args.input.tank_id),
        eq(tankages.organization_id, args.organizationId),
        measuredOnCondition,
      ),
    )
    .orderBy(desc(tankages.measured_at), desc(tankages.created_at));

  return schema.v1.tankage.listBy.tank.output.parse({ data: rows });
}

export async function createTankage(args: {
  input: z.infer<typeof schema.v1.tankage.create.input>;
  organizationId: string;
  ability: AppAbility;
  timezone: string;
  actorUserId: string;
  actorName: string;
}) {
  const [tank] = await db
    .select()
    .from(tanks)
    .where(
      and(
        eq(tanks.id, args.input.tank_id),
        eq(tanks.organization_id, args.organizationId),
      ),
    );
  if (!tank) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Tanque inválido",
    });
  }

  const timezone = args.timezone;
  const operationalDay = operationalDayKey(args.input.measured_at, timezone);
  const bulletinStatus = await getTankDayBulletinStatus({
    organizationId: args.organizationId,
    tankId: tank.id,
    operationalDay,
  });

  if (
    args.ability.cannot(
      "create",
      subject("Tankages", {
        organization_id: args.organizationId,
        bulletin_status: bulletinStatus,
      }),
    )
  ) {
    throw new ORPCError("FORBIDDEN", {
      message:
        bulletinStatus === "approved"
          ? "Boletim aprovado — não é possível alterar medições"
          : "Você não tem permissão para alterar medições",
    });
  }

  await assertTankDayMeasurementCapacity({
    organizationId: args.organizationId,
    tankId: tank.id,
    operationalDay,
  });

  const measurementEquipmentId =
    args.input.measurement_equipment_id === undefined
      ? tank.measurement_equipment_id
      : args.input.measurement_equipment_id;
  if (measurementEquipmentId != null) {
    const [equipment] = await db
      .select({ id: measurement_equipments.id })
      .from(measurement_equipments)
      .where(
        and(
          eq(measurement_equipments.id, measurementEquipmentId),
          eq(measurement_equipments.organization_id, args.organizationId),
        ),
      );
    if (!equipment) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Trena inválida",
      });
    }
  }

  const measurementContext = await loadTankageMeasurementContext({
    organizationId: args.organizationId,
    tankId: tank.id,
    measuredAt: args.input.measured_at,
    operationalDay,
  });
  assertTankageMeasurementWithinBounds({
    context: measurementContext,
    currentMeasurement: args.input.current_measurement,
    measuredAt: args.input.measured_at,
  });
  const previousMeasurement = measurementContext.previous_measurement;

  const volumeColumns = await computeTankageVolumeColumns({
    organizationId: args.organizationId,
    tankId: tank.id,
    operationalDay,
    measurement: {
      tank_id: tank.id,
      current_measurement: args.input.current_measurement,
      measured_at: args.input.measured_at,
      oil_temperature_c: args.input.oil_temperature_c,
      ambient_temperature_c: args.input.ambient_temperature_c,
    },
  });

  await assertTankageMeasurementStock({
    organizationId: args.organizationId,
    tankId: tank.id,
    operationalDay,
    measuredAt: args.input.measured_at,
    measurement: {
      tank_id: tank.id,
      current_measurement: args.input.current_measurement,
      measured_at: args.input.measured_at,
      oil_temperature_c: args.input.oil_temperature_c,
      ambient_temperature_c: args.input.ambient_temperature_c,
    },
    volumeColumns,
  });

  const now = new Date();
  const createdId = await db.transaction(async (tx) => {
    const [created] = await tx
      .insert(tankages)
      .values({
        tank_id: tank.id,
        concession_id: tank.concession_id,
        installation_id: tank.installation_id,
        measurement_equipment_id: measurementEquipmentId,
        operator_user_id: args.input.operator_user_id,
        measured_at: args.input.measured_at,
        operational_day: operationalDay,
        previous_measurement: previousMeasurement,
        current_measurement: args.input.current_measurement,
        oil_temperature_c: args.input.oil_temperature_c,
        ambient_temperature_c: args.input.ambient_temperature_c,
        observation: args.input.observation,
        latitude: args.input.latitude,
        longitude: args.input.longitude,
        organization_id: args.organizationId,
        created_by_user_id: args.actorUserId,
        ...volumeColumns,
      })
      .returning({ id: tankages.id });
    if (!created) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar tancagem",
      });
    }

    await recordAuditEvents(tx, [
      {
        organization_id: args.organizationId,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(tank.id, operationalDay),
        entity_type: "tankage",
        entity_id: created.id,
        action: "create",
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: {
          current_measurement: args.input.current_measurement,
          measured_at: args.input.measured_at.toISOString(),
        },
        occurred_at: now,
      },
    ]);

    return created.id;
  });

  const [row] = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(eq(tankages.id, createdId));
  if (!row) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao carregar a tancagem criada",
    });
  }
  return schema.v1.tankage.create.output.parse(row);
}

export async function updateTankage(args: {
  input: z.infer<typeof schema.v1.tankage.update.input>;
  ability: AppAbility;
  timezone: string;
  actorUserId: string;
  actorName: string;
}) {
  const { id, ...data } = args.input;
  const [existing] = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(eq(tankages.id, id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tancagem não encontrada",
    });
  }
  if (args.ability.cannot("update", subject("Tankages", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message:
        existing.bulletin_status === "approved"
          ? "Boletim aprovado — não é possível alterar medições"
          : "Você não tem permissão para atualizar esta tancagem",
    });
  }

  const timezone = args.timezone;
  const existingDayKey = existing.operational_day;

  const updateValues: Partial<typeof tankages.$inferInsert> = {
    operator_user_id: data.operator_user_id,
    previous_measurement: data.previous_measurement,
    current_measurement: data.current_measurement,
    measured_at: data.measured_at,
    oil_temperature_c: data.oil_temperature_c,
    ambient_temperature_c: data.ambient_temperature_c,
    observation: data.observation,
    measurement_equipment_id: data.measurement_equipment_id,
    latitude: data.latitude,
    longitude: data.longitude,
  };

  if (data.tank_id != null) {
    const [tank] = await db
      .select()
      .from(tanks)
      .where(
        and(
          eq(tanks.id, data.tank_id),
          eq(tanks.organization_id, existing.organization_id),
        ),
      );
    if (!tank) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Tanque inválido",
      });
    }
    updateValues.tank_id = tank.id;
    updateValues.concession_id = tank.concession_id;
    updateValues.installation_id = tank.installation_id;
  }

  if (data.measurement_equipment_id != null) {
    const [equipment] = await db
      .select({ id: measurement_equipments.id })
      .from(measurement_equipments)
      .where(
        and(
          eq(measurement_equipments.id, data.measurement_equipment_id),
          eq(measurement_equipments.organization_id, existing.organization_id),
        ),
      );
    if (!equipment) {
      throw new ORPCError("BAD_REQUEST", {
        message: "Trena inválida",
      });
    }
  }

  const nextTankId = updateValues.tank_id ?? existing.tank_id;
  const nextMeasuredAt = updateValues.measured_at ?? existing.measured_at;
  if (updateValues.measured_at != null) {
    updateValues.operational_day = operationalDayKey(nextMeasuredAt, timezone);
  }
  const nextDayKey = updateValues.operational_day ?? existing.operational_day;
  if (updateValues.measured_at != null && nextDayKey !== existingDayKey) {
    throw new ORPCError("BAD_REQUEST", {
      message: "A hora deve permanecer no mesmo dia operacional do boletim",
    });
  }
  if (nextDayKey !== existingDayKey || nextTankId !== existing.tank_id) {
    const nextBulletinStatus = await getTankDayBulletinStatus({
      organizationId: existing.organization_id,
      tankId: nextTankId,
      operationalDay: nextDayKey,
    });
    if (
      args.ability.cannot(
        "update",
        subject("Tankages", {
          ...existing,
          bulletin_status: nextBulletinStatus,
        }),
      )
    ) {
      throw new ORPCError("FORBIDDEN", {
        message:
          nextBulletinStatus === "approved"
            ? "Boletim aprovado — não é possível alterar medições"
            : "Você não tem permissão para alterar medições",
      });
    }
  }
  if (nextTankId !== existing.tank_id || nextDayKey !== existingDayKey) {
    await assertTankDayMeasurementCapacity({
      organizationId: existing.organization_id,
      tankId: nextTankId,
      operationalDay: nextDayKey,
      excludeId: id,
    });
  }

  const [transferRow] = await db
    .select({ id: tank_transfers.id })
    .from(tank_transfers)
    .where(eq(tank_transfers.tankage_id, id))
    .limit(1);
  const isTransfer = transferRow != null;

  const measurementContext = await loadTankageMeasurementContext({
    organizationId: existing.organization_id,
    tankId: nextTankId,
    measuredAt: nextMeasuredAt,
    operationalDay: nextDayKey,
    excludeId: id,
  });
  assertTankageMeasurementWithinBounds({
    context: measurementContext,
    currentMeasurement:
      updateValues.current_measurement ?? existing.current_measurement,
    measuredAt: nextMeasuredAt,
    allowDecrease: isTransfer,
  });
  if (!isTransfer) {
    updateValues.previous_measurement = measurementContext.previous_measurement;
  }

  const mergedForVolume = {
    tank_id: nextTankId,
    current_measurement:
      updateValues.current_measurement ?? existing.current_measurement,
    measured_at: nextMeasuredAt,
    oil_temperature_c:
      updateValues.oil_temperature_c ?? existing.oil_temperature_c,
    ambient_temperature_c:
      updateValues.ambient_temperature_c ?? existing.ambient_temperature_c,
  };

  const volumeColumns = await computeTankageVolumeColumns({
    organizationId: existing.organization_id,
    tankId: nextTankId,
    operationalDay: nextDayKey,
    measurement: mergedForVolume,
  });

  await assertTankageMeasurementStock({
    organizationId: existing.organization_id,
    tankId: nextTankId,
    operationalDay: nextDayKey,
    measuredAt: nextMeasuredAt,
    measurement: mergedForVolume,
    volumeColumns,
    excludeId: id,
  });

  Object.assign(updateValues, volumeColumns);

  const afterSnapshot = {
    measured_at: nextMeasuredAt,
    current_measurement:
      updateValues.current_measurement ?? existing.current_measurement,
    oil_temperature_c:
      updateValues.oil_temperature_c ?? existing.oil_temperature_c,
    ambient_temperature_c:
      updateValues.ambient_temperature_c ?? existing.ambient_temperature_c,
    observation:
      updateValues.observation !== undefined
        ? updateValues.observation
        : existing.observation,
    operator_user_id:
      updateValues.operator_user_id ?? existing.operator_user_id,
    measurement_equipment_id:
      updateValues.measurement_equipment_id !== undefined
        ? updateValues.measurement_equipment_id
        : existing.measurement_equipment_id,
  };

  const changes = buildTankageAuditChanges({
    before: {
      measured_at: existing.measured_at,
      current_measurement: existing.current_measurement,
      oil_temperature_c: existing.oil_temperature_c,
      ambient_temperature_c: existing.ambient_temperature_c,
      observation: existing.observation,
      operator_user_id: existing.operator_user_id,
      measurement_equipment_id: existing.measurement_equipment_id,
    },
    after: afterSnapshot,
  });

  const now = new Date();
  await db.transaction(async (tx) => {
    await tx.update(tankages).set(updateValues).where(eq(tankages.id, id));
    if (changes.length > 0) {
      await recordAuditEvents(tx, [
        {
          organization_id: existing.organization_id,
          aggregate_type: "tank_day_bulletin",
          aggregate_id: tankDayAggregateId(nextTankId, nextDayKey),
          entity_type: "tankage",
          entity_id: id,
          action: "update",
          actor_user_id: args.actorUserId,
          actor_name: args.actorName,
          metadata: { changes },
          occurred_at: now,
        },
      ]);
    }
  });

  const [row] = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(eq(tankages.id, id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tancagem não encontrada",
    });
  }
  return schema.v1.tankage.update.output.parse(row);
}

export async function retreatTankage(args: {
  input: z.infer<typeof schema.v1.tankage.retreat.input>;
  ability: AppAbility;
  timezone: string;
  actorUserId: string;
  actorName: string;
}) {
  const { id, justification, ...data } = args.input;
  const [existing] = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(eq(tankages.id, id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tancagem não encontrada",
    });
  }
  if (args.ability.cannot("retreat", subject("Tankages", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message:
        existing.bulletin_status === "open"
          ? "Retratamento só é permitido em boletim aprovado"
          : "Você não tem permissão para retratar esta medição",
    });
  }

  const [transferRow] = await db
    .select({ id: tank_transfers.id })
    .from(tank_transfers)
    .where(eq(tank_transfers.tankage_id, id))
    .limit(1);
  if (transferRow != null) {
    throw new ORPCError("BAD_REQUEST", {
      message:
        "Esta medição pertence a uma transferência. Use o retratamento da transferência.",
    });
  }

  const updateValues: Partial<typeof tankages.$inferInsert> = {};
  if (data.measured_at != null) {
    updateValues.measured_at = data.measured_at;
  }
  if (data.current_measurement != null) {
    updateValues.current_measurement = data.current_measurement;
  }
  if (data.oil_temperature_c != null) {
    updateValues.oil_temperature_c = data.oil_temperature_c;
  }
  if (data.ambient_temperature_c != null) {
    updateValues.ambient_temperature_c = data.ambient_temperature_c;
  }
  if (data.observation != null) {
    updateValues.observation = data.observation;
  }

  const nextMeasuredAt = updateValues.measured_at ?? existing.measured_at;
  if (updateValues.measured_at != null) {
    updateValues.operational_day = operationalDayKey(
      nextMeasuredAt,
      args.timezone,
    );
  }
  const nextDayKey = updateValues.operational_day ?? existing.operational_day;
  if (nextDayKey !== existing.operational_day) {
    throw new ORPCError("BAD_REQUEST", {
      message: "A hora deve permanecer no mesmo dia operacional do boletim",
    });
  }

  const measurementContext = await loadTankageMeasurementContext({
    organizationId: existing.organization_id,
    tankId: existing.tank_id,
    measuredAt: nextMeasuredAt,
    operationalDay: nextDayKey,
    excludeId: id,
  });
  assertTankageMeasurementWithinBounds({
    context: measurementContext,
    currentMeasurement:
      updateValues.current_measurement ?? existing.current_measurement,
    measuredAt: nextMeasuredAt,
    allowDecrease: false,
  });
  updateValues.previous_measurement = measurementContext.previous_measurement;

  const mergedForVolume = {
    tank_id: existing.tank_id,
    current_measurement:
      updateValues.current_measurement ?? existing.current_measurement,
    measured_at: nextMeasuredAt,
    oil_temperature_c:
      updateValues.oil_temperature_c ?? existing.oil_temperature_c,
    ambient_temperature_c:
      updateValues.ambient_temperature_c ?? existing.ambient_temperature_c,
  };

  const volumeColumns = await computeTankageVolumeColumns({
    organizationId: existing.organization_id,
    tankId: existing.tank_id,
    operationalDay: nextDayKey,
    measurement: mergedForVolume,
  });

  await assertTankageMeasurementStock({
    organizationId: existing.organization_id,
    tankId: existing.tank_id,
    operationalDay: nextDayKey,
    measuredAt: nextMeasuredAt,
    measurement: mergedForVolume,
    volumeColumns,
    excludeId: id,
  });

  Object.assign(updateValues, volumeColumns);

  const afterSnapshot = {
    measured_at: nextMeasuredAt,
    current_measurement:
      updateValues.current_measurement ?? existing.current_measurement,
    oil_temperature_c:
      updateValues.oil_temperature_c ?? existing.oil_temperature_c,
    ambient_temperature_c:
      updateValues.ambient_temperature_c ?? existing.ambient_temperature_c,
    observation:
      updateValues.observation !== undefined
        ? updateValues.observation
        : existing.observation,
  };

  const changes = buildTankageAuditChanges({
    before: {
      measured_at: existing.measured_at,
      current_measurement: existing.current_measurement,
      oil_temperature_c: existing.oil_temperature_c,
      ambient_temperature_c: existing.ambient_temperature_c,
      observation: existing.observation,
    },
    after: afterSnapshot,
  });
  if (changes.length === 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Nenhuma alteração para registrar no retratamento",
    });
  }

  const now = new Date();
  await db.transaction(async (tx) => {
    await tx.update(tankages).set(updateValues).where(eq(tankages.id, id));
    await recordAuditEvents(tx, [
      {
        organization_id: existing.organization_id,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(existing.tank_id, nextDayKey),
        entity_type: "tankage",
        entity_id: id,
        action: "retreat",
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: { justification, changes },
        occurred_at: now,
      },
    ]);
  });

  const [row] = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(eq(tankages.id, id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tancagem não encontrada",
    });
  }
  return schema.v1.tankage.retreat.output.parse(row);
}

export async function deleteTankages(args: {
  input: { ids: string[] };
  ability: AppAbility;
  actorUserId: string;
  actorName: string;
}) {
  const rows = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(inArray(tankages.id, args.input.ids));
  if (rows.length === 0) {
    throw new ORPCError("NOT_FOUND", {
      message: "Nenhuma tancagem encontrada",
    });
  }
  const notFoundIds = args.input.ids.filter(
    (id) => !rows.some((r) => r.id === id),
  );
  if (notFoundIds.length > 0) {
    throw new ORPCError("NOT_FOUND", {
      message: `Tankages not found: ${notFoundIds.join(", ")}`,
    });
  }
  const unauthorized = rows.filter((r) =>
    args.ability.cannot("delete", subject("Tankages", r)),
  );
  if (unauthorized.length > 0) {
    throw new ORPCError("FORBIDDEN", {
      message: unauthorized.some((row) => row.bulletin_status === "approved")
        ? "Boletim aprovado — não é possível alterar medições"
        : "You do not have permission to delete one or more of these tankages",
    });
  }

  const now = new Date();
  await db.transaction(async (tx) => {
    await tx.delete(tankages).where(inArray(tankages.id, args.input.ids));
    await recordAuditEvents(
      tx,
      rows.map((row) => ({
        organization_id: row.organization_id,
        aggregate_type: "tank_day_bulletin" as const,
        aggregate_id: tankDayAggregateId(row.tank_id, row.operational_day),
        entity_type: "tankage" as const,
        entity_id: row.id,
        action: "delete" as const,
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: {
          current_measurement: row.current_measurement,
          measured_at: row.measured_at.toISOString(),
        },
        occurred_at: now,
      })),
    );
  });
  return null;
}

export async function getSelectedTankages(args: {
  input: z.infer<typeof schema.v1.tankage.get.selected.input>;
  organizationId: string;
  timezone: string;
}) {
  const isSelectAll = args.input.all === true;

  const explicitIds: string[] = isSelectAll
    ? []
    : args.input.selection
      ? extractExplicitIds(args.input.selection)
      : [];

  if (!isSelectAll && explicitIds.length === 0) {
    return [];
  }

  const clientTimezone = args.timezone;
  const excludeIds = args.input.exclude ?? [];
  const whereClause = isSelectAll
    ? buildWhereClause({
        organizationId: args.organizationId,
        columnFilters: args.input.columnFilters,
        globalFilter: args.input.globalFilter,
        clientTimezone,
        excludeIds,
      })
    : and(
        eq(tankages.organization_id, args.organizationId),
        inArray(tankages.id, explicitIds),
      );

  const effectiveOrderBy = isSelectAll
    ? buildOrderBy(args.input.sorting ?? [])
    : [asc(tankages.id)];

  const rows = await db
    .select(tankageOutputColumns)
    .from(tankages)
    .innerJoin(tanks, eq(tankages.tank_id, tanks.id))
    .innerJoin(concessions, eq(tankages.concession_id, concessions.id))
    .innerJoin(installations, eq(tankages.installation_id, installations.id))
    .innerJoin(users, eq(tankages.operator_user_id, users.id))
    .leftJoin(
      measurement_equipments,
      eq(tankages.measurement_equipment_id, measurement_equipments.id),
    )
    .leftJoin(tank_day_bulletins, tankageBulletinJoin)
    .where(whereClause)
    .orderBy(...effectiveOrderBy);

  return schema.v1.tankage.get.selected.output.parse(rows);
}
function parseArrayFilter(value: unknown): string[] | null {
  if (Array.isArray(value)) {
    const filtered = value.filter((v): v is string => typeof v === "string");
    return filtered.length > 0 ? filtered : null;
  }
  if (typeof value === "string" && value.length > 0) {
    return [value];
  }
  return null;
}

function isTankageColumn(id: string): id is keyof typeof tankageOutputColumns {
  return id in tankageOutputColumns;
}

function extractExplicitIds(selection: RowSelection): string[] {
  const ids: string[] = [];
  for (const [id, selected] of Object.entries(selection)) {
    if (selected) {
      ids.push(id);
    }
  }
  return ids;
}

interface WhereClauseParams {
  organizationId: string;
  columnFilters?: ColumnFilters;
  globalFilter?: GlobalFilter;
  clientTimezone: string;
  excludeIds?: string[];
}

function buildWhereClause({
  organizationId,
  columnFilters = [],
  globalFilter,
  clientTimezone,
  excludeIds = [],
}: WhereClauseParams): SQL | undefined {
  const baseFilter = eq(tankages.organization_id, organizationId);

  const excludeCondition =
    excludeIds.length > 0 ? not(inArray(tankages.id, excludeIds)) : undefined;

  const globalFilterCondition = globalFilter
    ? or(
        ilike(tanks.tag, `%${globalFilter}%`),
        ilike(concessions.name, `%${globalFilter}%`),
        ilike(installations.name, `%${globalFilter}%`),
        ilike(users.name, `%${globalFilter}%`),
      )
    : undefined;

  const dynamicFilters = columnFilters.flatMap((f) => {
    if (!isTankageColumn(f.id)) return [];
    const condition: SQL | undefined = (() => {
      switch (f.id) {
        case "tag":
          if (typeof f.value !== "string") return undefined;
          return like(tanks.tag, `%${f.value}%`);
        case "concession_id": {
          const values = parseArrayFilter(f.value);
          if (!values) return undefined;
          return inArray(tankages.concession_id, values);
        }
        case "installation_id": {
          const values = parseArrayFilter(f.value);
          if (!values) return undefined;
          return inArray(tankages.installation_id, values);
        }
        case "operator_user_id": {
          const values = parseArrayFilter(f.value);
          if (!values) return undefined;
          return inArray(tankages.operator_user_id, values);
        }
        case "measured_at": {
          const parsed = parseDateFilterValue(f.value);
          if (!parsed) return undefined;
          const ranges = Array.isArray(parsed) ? parsed : [parsed];
          const conditions = ranges
            .map((p) =>
              dateFilterToCondition(tankages.measured_at, p, {
                clientTimezone,
              }),
            )
            .filter((c): c is SQL => c !== undefined);
          return conditions.length === 0
            ? undefined
            : conditions.length === 1
              ? conditions[0]
              : or(...conditions);
        }
        case "created_at": {
          const parsed = parseDateFilterValue(f.value);
          if (!parsed) return undefined;
          const ranges = Array.isArray(parsed) ? parsed : [parsed];
          const conditions = ranges
            .map((p) =>
              dateFilterToCondition(tankages.created_at, p, {
                clientTimezone,
              }),
            )
            .filter((c): c is SQL => c !== undefined);
          return conditions.length === 0
            ? undefined
            : conditions.length === 1
              ? conditions[0]
              : or(...conditions);
        }
        case "updated_at": {
          const parsed = parseDateFilterValue(f.value);
          if (!parsed) return undefined;
          const ranges = Array.isArray(parsed) ? parsed : [parsed];
          const conditions = ranges
            .map((p) =>
              dateFilterToCondition(tankages.updated_at, p, {
                clientTimezone,
              }),
            )
            .filter((c): c is SQL => c !== undefined);
          return conditions.length === 0
            ? undefined
            : conditions.length === 1
              ? conditions[0]
              : or(...conditions);
        }
        default:
          return undefined;
      }
    })();
    return condition ? [condition] : [];
  });

  return and(
    baseFilter,
    excludeCondition,
    globalFilterCondition,
    ...dynamicFilters,
  );
}

function buildOrderBy(sorting: Sorting): ReturnType<typeof asc>[] {
  const orderByClause = sorting
    .map((s) => {
      if (!isTankageColumn(s.id)) return undefined;
      switch (s.id) {
        case "tag":
          return s.desc ? desc(tanks.tag) : asc(tanks.tag);
        case "previous_measurement":
          return s.desc
            ? desc(tankages.previous_measurement)
            : asc(tankages.previous_measurement);
        case "current_measurement":
          return s.desc
            ? desc(tankages.current_measurement)
            : asc(tankages.current_measurement);
        case "measured_at":
          return s.desc
            ? desc(tankages.measured_at)
            : asc(tankages.measured_at);
        case "created_at":
          return s.desc ? desc(tankages.id) : asc(tankages.id);
        case "updated_at":
          return s.desc ? desc(tankages.updated_at) : asc(tankages.updated_at);
        default:
          return undefined;
      }
    })
    .filter((x): x is ReturnType<typeof asc> => x !== undefined);

  return orderByClause.length > 0 ? orderByClause : [asc(tankages.id)];
}
