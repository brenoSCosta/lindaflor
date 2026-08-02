import {
  recordAuditEvents,
  tankDayAggregateId,
} from "@lindaflor/core/lib/audit/record";
import { getTankDayBulletinStatus } from "@lindaflor/core/tankage/day-bulletin";
import { loadVolumeContextForTank } from "@lindaflor/core/tankage/tank-volume-context";
import {
  assertTankageMeasurementStock,
  sumTankTransferOutflowGrossM3,
} from "@lindaflor/core/tankage/tankage-stock-validation";
import { db } from "@lindaflor/db";
import {
  measurement_equipments,
  tank_day_bulletins,
  tank_transfers,
  tankages,
  tanks,
} from "@lindaflor/db/schema/tankage";
import {
  resolveLabAnalysis,
  volumeAuditFromMeasurement,
} from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { computeTransferVolumes } from "@lindaflor/shared/functions/tankage/tank-transfer-volume";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import { buildTransferAuditChanges } from "@lindaflor/shared/lib/audit/transfer-changes";
import { operationalDayKey } from "@lindaflor/shared/lib/zoned-datetime";
import { schema } from "@lindaflor/shared/schemas/tankage";
import { ORPCError } from "@orpc/server";
import { and, asc, desc, eq, lt, ne, sql } from "drizzle-orm";
import type { z } from "zod";

const transferBulletinJoin = and(
  eq(tank_day_bulletins.organization_id, tank_transfers.organization_id),
  eq(tank_day_bulletins.tank_id, tank_transfers.tank_id),
  eq(tank_day_bulletins.operational_day, tank_transfers.operational_day),
);

const transferOutputColumns = {
  id: tank_transfers.id,
  tank_id: tank_transfers.tank_id,
  organization_id: tank_transfers.organization_id,
  operational_day: tank_transfers.operational_day,
  transferred_at: tank_transfers.transferred_at,
  height_before_m: tank_transfers.height_before_m,
  height_after_m: tank_transfers.height_after_m,
  oil_temperature_c: tank_transfers.oil_temperature_c,
  ambient_temperature_c: tank_transfers.ambient_temperature_c,
  gross_volume_before_m3: tank_transfers.gross_volume_before_m3,
  gross_volume_after_m3: tank_transfers.gross_volume_after_m3,
  gross_volume_out_m3: tank_transfers.gross_volume_out_m3,
  gross_volume_out_m3_20c: tank_transfers.gross_volume_out_m3_20c,
  net_oil_volume_out_m3_20c: tank_transfers.net_oil_volume_out_m3_20c,
  shell_temperature_c: tank_transfers.shell_temperature_c,
  shell_correction_factor: tank_transfers.shell_correction_factor,
  liquid_correction_factor: tank_transfers.liquid_correction_factor,
  combined_correction_factor: tank_transfers.combined_correction_factor,
  tank_calibration_id: tank_transfers.tank_calibration_id,
  lab_oil_analysis_id: tank_transfers.lab_oil_analysis_id,
  density_at_20c_kg_m3: tank_transfers.density_at_20c_kg_m3,
  water_and_sediment_percent: tank_transfers.water_and_sediment_percent,
  destination_label: tank_transfers.destination_label,
  observation: tank_transfers.observation,
  tankage_id: tank_transfers.tankage_id,
  created_by_user_id: tank_transfers.created_by_user_id,
  created_at: tank_transfers.created_at,
  updated_at: tank_transfers.updated_at,
  bulletin_status: sql<
    "open" | "approved"
  >`coalesce(${tank_day_bulletins.status}, 'open')`.as("bulletin_status"),
};

export async function listTransfersByTank(args: {
  input: { tank_id: string; operational_day?: string | null };
  organizationId: string;
}) {
  const [tank] = await db
    .select({ id: tanks.id })
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

  const rows = await db
    .select(transferOutputColumns)
    .from(tank_transfers)
    .leftJoin(tank_day_bulletins, transferBulletinJoin)
    .where(
      and(
        eq(tank_transfers.tank_id, args.input.tank_id),
        eq(tank_transfers.organization_id, args.organizationId),
        args.input.operational_day != null
          ? eq(tank_transfers.operational_day, args.input.operational_day)
          : undefined,
      ),
    )
    .orderBy(asc(tank_transfers.transferred_at));

  return schema.v1.transfer.listBy.tank.output.parse({ data: rows });
}

export async function createTransfer(args: {
  input: z.infer<typeof schema.v1.transfer.create.input>;
  organizationId: string;
  ability: AppAbility;
  timezone: string;
  createdByUserId: string;
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
    throw new ORPCError("BAD_REQUEST", { message: "Tanque inválido" });
  }

  const operationalDay = operationalDayKey(
    args.input.transferred_at,
    args.timezone,
  );
  const bulletinStatus = await getTankDayBulletinStatus({
    organizationId: args.organizationId,
    tankId: tank.id,
    operationalDay,
  });

  if (
    args.ability.cannot(
      "create",
      subject("TankTransfers", {
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
      throw new ORPCError("BAD_REQUEST", { message: "Trena inválida" });
    }
  }

  const volumeContext = await loadVolumeContextForTank({
    organizationId: args.organizationId,
    tankId: tank.id,
    calibrationDayKey: operationalDay,
  });
  const points = volumeContext.calibrationPointsByTank.get(tank.id) ?? [];
  const calibrationId = volumeContext.calibrationIdByTank.get(tank.id) ?? null;
  const lab = resolveLabAnalysis(
    volumeContext.labByTank.get(tank.id),
    args.input.transferred_at,
  );

  const transferVolumes = computeTransferVolumes({
    calibrationPoints: points,
    height_before_m: args.input.height_before_m,
    height_after_m: args.input.height_after_m,
    oil_temperature_c: args.input.oil_temperature_c,
    ambient_temperature_c: args.input.ambient_temperature_c,
    lab:
      lab == null
        ? null
        : {
            id: lab.id,
            density_at_20c: lab.density_at_20c,
            water_and_sediment_percent: lab.water_and_sediment_percent,
          },
  });
  if (!transferVolumes.ok) {
    throw new ORPCError("BAD_REQUEST", {
      message:
        transferVolumes.code === "NO_OUTFLOW"
          ? "A altura depois deve ser menor que a altura antes"
          : "Altura fora da tabela de arqueação vigente para esta data",
    });
  }

  const afterMeasurement = {
    tank_id: tank.id,
    current_measurement: args.input.height_after_m,
    measured_at: args.input.transferred_at,
    oil_temperature_c: args.input.oil_temperature_c,
    ambient_temperature_c: args.input.ambient_temperature_c,
  };
  const afterVolumeColumns = volumeAuditFromMeasurement({
    measurement: afterMeasurement,
    calibrationPoints: points,
    tankCalibrationId: calibrationId,
    labAnalyses: volumeContext.labByTank.get(tank.id),
  });

  const [prior] = await db
    .select({ measured_at: tankages.measured_at })
    .from(tankages)
    .where(
      and(
        eq(tankages.tank_id, tank.id),
        eq(tankages.organization_id, args.organizationId),
        lt(tankages.measured_at, args.input.transferred_at),
      ),
    )
    .orderBy(desc(tankages.measured_at), desc(tankages.created_at))
    .limit(1);

  const priorOutflow = await sumTankTransferOutflowGrossM3({
    organizationId: args.organizationId,
    tankId: tank.id,
    untilMeasuredAt: args.input.transferred_at,
    afterMeasuredAt: prior?.measured_at ?? null,
  });

  await assertTankageMeasurementStock({
    organizationId: args.organizationId,
    tankId: tank.id,
    operationalDay,
    measuredAt: args.input.transferred_at,
    measurement: afterMeasurement,
    volumeColumns: afterVolumeColumns,
    documentedOutflowGrossM3:
      priorOutflow + transferVolumes.gross_volume_out_m3,
  });

  const destinationLabel =
    args.input.destination_label == null ||
    args.input.destination_label.trim() === ""
      ? null
      : args.input.destination_label.trim();

  const created = await db.transaction(async (tx) => {
    const [tankageRow] = await tx
      .insert(tankages)
      .values({
        tank_id: tank.id,
        concession_id: tank.concession_id,
        installation_id: tank.installation_id,
        measurement_equipment_id: measurementEquipmentId,
        operator_user_id: args.input.operator_user_id,
        measured_at: args.input.transferred_at,
        operational_day: operationalDay,
        previous_measurement: args.input.height_before_m,
        current_measurement: args.input.height_after_m,
        oil_temperature_c: args.input.oil_temperature_c,
        ambient_temperature_c: args.input.ambient_temperature_c,
        observation: args.input.observation,
        organization_id: args.organizationId,
        created_by_user_id: args.createdByUserId,
        ...afterVolumeColumns,
      })
      .returning({ id: tankages.id });
    if (!tankageRow) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar medição da transferência",
      });
    }

    const [transferRow] = await tx
      .insert(tank_transfers)
      .values({
        tank_id: tank.id,
        organization_id: args.organizationId,
        operational_day: operationalDay,
        transferred_at: args.input.transferred_at,
        height_before_m: args.input.height_before_m,
        height_after_m: args.input.height_after_m,
        oil_temperature_c: args.input.oil_temperature_c,
        ambient_temperature_c: args.input.ambient_temperature_c,
        gross_volume_before_m3: transferVolumes.gross_volume_before_m3,
        gross_volume_after_m3: transferVolumes.gross_volume_after_m3,
        gross_volume_out_m3: transferVolumes.gross_volume_out_m3,
        gross_volume_out_m3_20c: transferVolumes.gross_volume_out_m3_20c,
        net_oil_volume_out_m3_20c: transferVolumes.net_oil_volume_out_m3_20c,
        shell_temperature_c: transferVolumes.shell_temperature_c,
        shell_correction_factor: transferVolumes.shell_correction_factor,
        liquid_correction_factor: transferVolumes.liquid_correction_factor,
        combined_correction_factor: transferVolumes.combined_correction_factor,
        tank_calibration_id: calibrationId,
        lab_oil_analysis_id: transferVolumes.lab_oil_analysis_id,
        density_at_20c_kg_m3: transferVolumes.density_at_20c_kg_m3,
        water_and_sediment_percent: transferVolumes.water_and_sediment_percent,
        destination_label: destinationLabel,
        observation: args.input.observation,
        tankage_id: tankageRow.id,
        created_by_user_id: args.createdByUserId,
      })
      .returning();
    if (!transferRow) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar transferência",
      });
    }

    await recordAuditEvents(tx, [
      {
        organization_id: args.organizationId,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(tank.id, operationalDay),
        entity_type: "tank_transfer",
        entity_id: transferRow.id,
        action: "create",
        actor_user_id: args.createdByUserId,
        actor_name: args.actorName,
        metadata: {
          height_before_m: args.input.height_before_m,
          height_after_m: args.input.height_after_m,
          gross_volume_out_m3: transferVolumes.gross_volume_out_m3,
          destination_label: destinationLabel,
          tankage_id: tankageRow.id,
        },
        occurred_at: new Date(),
      },
    ]);

    return transferRow;
  });

  return schema.v1.transfer.create.output.parse({
    ...created,
    bulletin_status: bulletinStatus,
  });
}

export async function retreatTransfer(args: {
  input: z.infer<typeof schema.v1.transfer.retreat.input>;
  ability: AppAbility;
  timezone: string;
  actorUserId: string;
  actorName: string;
}) {
  const { id, justification, ...data } = args.input;
  const [existing] = await db
    .select(transferOutputColumns)
    .from(tank_transfers)
    .leftJoin(tank_day_bulletins, transferBulletinJoin)
    .where(eq(tank_transfers.id, id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Transferência não encontrada",
    });
  }
  if (args.ability.cannot("retreat", subject("TankTransfers", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message:
        existing.bulletin_status === "open"
          ? "Retratamento só é permitido em boletim aprovado"
          : "Você não tem permissão para retratar esta transferência",
    });
  }

  const nextTransferredAt = data.transferred_at ?? existing.transferred_at;
  const nextDayKey = operationalDayKey(nextTransferredAt, args.timezone);
  if (nextDayKey !== existing.operational_day) {
    throw new ORPCError("BAD_REQUEST", {
      message: "A hora deve permanecer no mesmo dia operacional do boletim",
    });
  }

  const heightBefore = data.height_before_m ?? existing.height_before_m;
  const heightAfter = data.height_after_m ?? existing.height_after_m;
  const oilTemp = data.oil_temperature_c ?? existing.oil_temperature_c;
  const ambientTemp =
    data.ambient_temperature_c ?? existing.ambient_temperature_c;
  const observation = data.observation ?? existing.observation;
  const destinationLabel =
    data.destination_label === undefined
      ? existing.destination_label
      : data.destination_label == null || data.destination_label.trim() === ""
        ? null
        : data.destination_label.trim();

  const volumeContext = await loadVolumeContextForTank({
    organizationId: existing.organization_id,
    tankId: existing.tank_id,
    calibrationDayKey: nextDayKey,
  });
  const points =
    volumeContext.calibrationPointsByTank.get(existing.tank_id) ?? [];
  const calibrationId =
    volumeContext.calibrationIdByTank.get(existing.tank_id) ?? null;
  const lab = resolveLabAnalysis(
    volumeContext.labByTank.get(existing.tank_id),
    nextTransferredAt,
  );

  const transferVolumes = computeTransferVolumes({
    calibrationPoints: points,
    height_before_m: heightBefore,
    height_after_m: heightAfter,
    oil_temperature_c: oilTemp,
    ambient_temperature_c: ambientTemp,
    lab:
      lab == null
        ? null
        : {
            id: lab.id,
            density_at_20c: lab.density_at_20c,
            water_and_sediment_percent: lab.water_and_sediment_percent,
          },
  });
  if (!transferVolumes.ok) {
    throw new ORPCError("BAD_REQUEST", {
      message:
        transferVolumes.code === "NO_OUTFLOW"
          ? "A altura depois deve ser menor que a altura antes"
          : "Altura fora da tabela de arqueação vigente para esta data",
    });
  }

  const afterMeasurement = {
    tank_id: existing.tank_id,
    current_measurement: heightAfter,
    measured_at: nextTransferredAt,
    oil_temperature_c: oilTemp,
    ambient_temperature_c: ambientTemp,
  };
  const afterVolumeColumns = volumeAuditFromMeasurement({
    measurement: afterMeasurement,
    calibrationPoints: points,
    tankCalibrationId: calibrationId,
    labAnalyses: volumeContext.labByTank.get(existing.tank_id),
  });

  const [prior] = await db
    .select({ measured_at: tankages.measured_at })
    .from(tankages)
    .where(
      and(
        eq(tankages.tank_id, existing.tank_id),
        eq(tankages.organization_id, existing.organization_id),
        lt(tankages.measured_at, nextTransferredAt),
        ne(tankages.id, existing.tankage_id),
      ),
    )
    .orderBy(desc(tankages.measured_at), desc(tankages.created_at))
    .limit(1);

  const priorOutflow = await sumTankTransferOutflowGrossM3({
    organizationId: existing.organization_id,
    tankId: existing.tank_id,
    untilMeasuredAt: nextTransferredAt,
    afterMeasuredAt: prior?.measured_at ?? null,
    excludeTransferId: existing.id,
  });

  await assertTankageMeasurementStock({
    organizationId: existing.organization_id,
    tankId: existing.tank_id,
    operationalDay: nextDayKey,
    measuredAt: nextTransferredAt,
    measurement: afterMeasurement,
    volumeColumns: afterVolumeColumns,
    excludeId: existing.tankage_id,
    documentedOutflowGrossM3:
      priorOutflow + transferVolumes.gross_volume_out_m3,
  });

  const changes = buildTransferAuditChanges({
    before: {
      transferred_at: existing.transferred_at,
      height_before_m: existing.height_before_m,
      height_after_m: existing.height_after_m,
      oil_temperature_c: existing.oil_temperature_c,
      ambient_temperature_c: existing.ambient_temperature_c,
      destination_label: existing.destination_label,
      observation: existing.observation,
    },
    after: {
      transferred_at: nextTransferredAt,
      height_before_m: heightBefore,
      height_after_m: heightAfter,
      oil_temperature_c: oilTemp,
      ambient_temperature_c: ambientTemp,
      destination_label: destinationLabel,
      observation,
    },
  });
  if (changes.length === 0) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Nenhuma alteração para registrar no retratamento",
    });
  }

  const now = new Date();
  await db.transaction(async (tx) => {
    await tx
      .update(tankages)
      .set({
        measured_at: nextTransferredAt,
        operational_day: nextDayKey,
        previous_measurement: heightBefore,
        current_measurement: heightAfter,
        oil_temperature_c: oilTemp,
        ambient_temperature_c: ambientTemp,
        observation,
        ...afterVolumeColumns,
      })
      .where(eq(tankages.id, existing.tankage_id));

    await tx
      .update(tank_transfers)
      .set({
        transferred_at: nextTransferredAt,
        operational_day: nextDayKey,
        height_before_m: heightBefore,
        height_after_m: heightAfter,
        oil_temperature_c: oilTemp,
        ambient_temperature_c: ambientTemp,
        gross_volume_before_m3: transferVolumes.gross_volume_before_m3,
        gross_volume_after_m3: transferVolumes.gross_volume_after_m3,
        gross_volume_out_m3: transferVolumes.gross_volume_out_m3,
        gross_volume_out_m3_20c: transferVolumes.gross_volume_out_m3_20c,
        net_oil_volume_out_m3_20c: transferVolumes.net_oil_volume_out_m3_20c,
        shell_temperature_c: transferVolumes.shell_temperature_c,
        shell_correction_factor: transferVolumes.shell_correction_factor,
        liquid_correction_factor: transferVolumes.liquid_correction_factor,
        combined_correction_factor: transferVolumes.combined_correction_factor,
        tank_calibration_id: calibrationId,
        lab_oil_analysis_id: transferVolumes.lab_oil_analysis_id,
        density_at_20c_kg_m3: transferVolumes.density_at_20c_kg_m3,
        water_and_sediment_percent: transferVolumes.water_and_sediment_percent,
        destination_label: destinationLabel,
        observation,
      })
      .where(eq(tank_transfers.id, existing.id));

    await recordAuditEvents(tx, [
      {
        organization_id: existing.organization_id,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(existing.tank_id, nextDayKey),
        entity_type: "tank_transfer",
        entity_id: existing.id,
        action: "retreat",
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: { justification, changes },
        occurred_at: now,
      },
    ]);
  });

  const [row] = await db
    .select(transferOutputColumns)
    .from(tank_transfers)
    .leftJoin(tank_day_bulletins, transferBulletinJoin)
    .where(eq(tank_transfers.id, existing.id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Transferência não encontrada",
    });
  }
  return schema.v1.transfer.retreat.output.parse(row);
}

export async function deleteTransfer(args: {
  input: { id: string };
  ability: AppAbility;
  actorUserId: string;
  actorName: string;
}) {
  const [existing] = await db
    .select(transferOutputColumns)
    .from(tank_transfers)
    .leftJoin(tank_day_bulletins, transferBulletinJoin)
    .where(eq(tank_transfers.id, args.input.id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Transferência não encontrada",
    });
  }
  if (args.ability.cannot("delete", subject("TankTransfers", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message:
        existing.bulletin_status === "approved"
          ? "Boletim aprovado — não é possível alterar medições"
          : "Você não tem permissão para excluir esta transferência",
    });
  }

  await db.transaction(async (tx) => {
    await tx.delete(tank_transfers).where(eq(tank_transfers.id, existing.id));
    await tx.delete(tankages).where(eq(tankages.id, existing.tankage_id));
    await recordAuditEvents(tx, [
      {
        organization_id: existing.organization_id,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(
          existing.tank_id,
          existing.operational_day,
        ),
        entity_type: "tank_transfer",
        entity_id: existing.id,
        action: "delete",
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: {
          height_before_m: existing.height_before_m,
          height_after_m: existing.height_after_m,
          gross_volume_out_m3: existing.gross_volume_out_m3,
          destination_label: existing.destination_label,
          tankage_id: existing.tankage_id,
        },
        occurred_at: new Date(),
      },
    ]);
  });

  return null;
}
