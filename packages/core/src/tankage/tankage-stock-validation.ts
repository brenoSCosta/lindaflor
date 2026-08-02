import { loadVolumeContextForTank } from "@lindaflor/core/tankage/tank-volume-context";
import { db } from "@lindaflor/db";
import { tank_transfers, tankages } from "@lindaflor/db/schema/tankage";
import type { CalibrationPoint } from "@lindaflor/shared/functions/tankage/calibration-volume";
import {
  measurementVolumesFromPersisted,
  volumesFromMeasurement,
  type MeasurementForVolume,
  type MeasurementVolumePersisted,
} from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { ORPCError } from "@orpc/server";
import { and, desc, eq, gt, lt, lte, ne, or, sql } from "drizzle-orm";

const VOLUME_EPSILON_M3 = 1e-6;

export function capacityGrossM3FromPoints(
  points: readonly CalibrationPoint[],
): number | null {
  if (points.length === 0) {
    return null;
  }
  return Math.max(...points.map((p) => p.volume_m3));
}

/**
 * Validates gross inventory implied by a new reading against prior stock and tank capacity.
 * When `documentedOutflowGrossM3` is set (including 0), withdrawals above that amount fail.
 */
export function assertInventoryChangeWithinBounds(args: {
  priorGrossM3: number | null;
  newGrossM3: number | null;
  capacityGrossM3: number | null;
  hasCalibration: boolean;
  /** When set, withdrawals above this require registered transferência / retratamento. */
  documentedOutflowGrossM3?: number;
}): void {
  const documentedOutflowGrossM3 = args.documentedOutflowGrossM3;

  if (args.hasCalibration && args.newGrossM3 == null) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Altura fora da tabela de arqueação vigente para esta data",
    });
  }

  if (args.newGrossM3 == null) {
    return;
  }

  if (args.newGrossM3 < -VOLUME_EPSILON_M3) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Volume bruto da medição não pode ser negativo",
    });
  }

  if (
    args.capacityGrossM3 != null &&
    args.newGrossM3 > args.capacityGrossM3 + VOLUME_EPSILON_M3
  ) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Volume bruto excede a capacidade do tanque",
    });
  }

  if (args.priorGrossM3 == null) {
    return;
  }

  const withdrawal = args.priorGrossM3 - args.newGrossM3;
  if (withdrawal <= VOLUME_EPSILON_M3) {
    return;
  }

  if (withdrawal > args.priorGrossM3 + VOLUME_EPSILON_M3) {
    throw new ORPCError("BAD_REQUEST", {
      message:
        "Não é possível retirar mais volume do que o estoque bruto disponível no tanque",
    });
  }

  if (
    documentedOutflowGrossM3 != null &&
    withdrawal > documentedOutflowGrossM3 + VOLUME_EPSILON_M3
  ) {
    throw new ORPCError("BAD_REQUEST", {
      message:
        "Redução de estoque superior ao volume documentado em transferências ou retratamentos. Registre a operação de saída antes de confirmar a medição.",
    });
  }
}

type PriorMeasurementRow = {
  id: string;
  operational_day: string;
  measured_at: Date;
  current_measurement: number;
  oil_temperature_c: number;
  ambient_temperature_c: number;
  gross_volume_m3: number | null;
  net_oil_volume_m3_20c: number | null;
  volume_oil_barrels: number | null;
};

async function loadPriorMeasurement(args: {
  organizationId: string;
  tankId: string;
  measuredAt: Date;
  excludeId?: string;
}): Promise<PriorMeasurementRow | null> {
  const filters = [
    eq(tankages.organization_id, args.organizationId),
    eq(tankages.tank_id, args.tankId),
  ];

  if (args.excludeId != null) {
    filters.push(ne(tankages.id, args.excludeId));
    const chronologyFilter = or(
      lt(tankages.measured_at, args.measuredAt),
      and(
        eq(tankages.measured_at, args.measuredAt),
        ne(tankages.id, args.excludeId),
      ),
    );
    if (chronologyFilter != null) {
      filters.push(chronologyFilter);
    }
  } else {
    filters.push(lt(tankages.measured_at, args.measuredAt));
  }

  const [row] = await db
    .select({
      id: tankages.id,
      operational_day: tankages.operational_day,
      measured_at: tankages.measured_at,
      current_measurement: tankages.current_measurement,
      oil_temperature_c: tankages.oil_temperature_c,
      ambient_temperature_c: tankages.ambient_temperature_c,
      gross_volume_m3: tankages.gross_volume_m3,
      net_oil_volume_m3_20c: tankages.net_oil_volume_m3_20c,
      volume_oil_barrels: tankages.volume_oil_barrels,
    })
    .from(tankages)
    .where(and(...filters))
    .orderBy(desc(tankages.measured_at), desc(tankages.created_at))
    .limit(1);

  return row ?? null;
}

async function resolveGrossM3(args: {
  organizationId: string;
  tankId: string;
  operationalDay: string;
  measurement: MeasurementForVolume;
  persisted: MeasurementVolumePersisted;
}): Promise<number | null> {
  const fromPersisted = measurementVolumesFromPersisted(args.persisted);
  if (fromPersisted != null) {
    return fromPersisted.gross_volume_m3;
  }

  const context = await loadVolumeContextForTank({
    organizationId: args.organizationId,
    tankId: args.tankId,
    calibrationDayKey: args.operationalDay,
  });
  const points = context.calibrationPointsByTank.get(args.tankId) ?? [];
  const lab = context.labByTank.get(args.tankId);
  const volumes = volumesFromMeasurement({
    measurement: args.measurement,
    calibrationPoints: points,
    labAnalyses: lab,
  });
  return volumes?.gross_volume_m3 ?? null;
}

/**
 * Sum ambient gross outflow from transfers strictly after `afterMeasuredAt`
 * (exclusive) through `untilMeasuredAt` (inclusive).
 */
export async function sumTankTransferOutflowGrossM3(args: {
  organizationId: string;
  tankId: string;
  untilMeasuredAt: Date;
  afterMeasuredAt?: Date | null;
  excludeTransferId?: string;
}): Promise<number> {
  const filters = [
    eq(tank_transfers.organization_id, args.organizationId),
    eq(tank_transfers.tank_id, args.tankId),
    lte(tank_transfers.transferred_at, args.untilMeasuredAt),
  ];
  if (args.afterMeasuredAt != null) {
    filters.push(gt(tank_transfers.transferred_at, args.afterMeasuredAt));
  }
  if (args.excludeTransferId != null) {
    filters.push(ne(tank_transfers.id, args.excludeTransferId));
  }

  const [row] = await db
    .select({
      total: sql<number>`coalesce(sum(${tank_transfers.gross_volume_out_m3}), 0)::float8`,
    })
    .from(tank_transfers)
    .where(and(...filters));

  return row?.total ?? 0;
}

export async function assertTankageMeasurementStock(args: {
  organizationId: string;
  tankId: string;
  operationalDay: string;
  measuredAt: Date;
  measurement: MeasurementForVolume;
  volumeColumns: MeasurementVolumePersisted;
  excludeId?: string;
  documentedOutflowGrossM3?: number;
}): Promise<void> {
  const context = await loadVolumeContextForTank({
    organizationId: args.organizationId,
    tankId: args.tankId,
    calibrationDayKey: args.operationalDay,
  });
  const points = context.calibrationPointsByTank.get(args.tankId) ?? [];
  const hasCalibration = points.length > 0;
  const capacityGrossM3 = capacityGrossM3FromPoints(points);

  const [newGrossM3, prior] = await Promise.all([
    resolveGrossM3({
      organizationId: args.organizationId,
      tankId: args.tankId,
      operationalDay: args.operationalDay,
      measurement: args.measurement,
      persisted: args.volumeColumns,
    }),
    loadPriorMeasurement({
      organizationId: args.organizationId,
      tankId: args.tankId,
      measuredAt: args.measuredAt,
      excludeId: args.excludeId,
    }),
  ]);

  let priorGrossM3: number | null = null;
  if (prior != null) {
    priorGrossM3 = await resolveGrossM3({
      organizationId: args.organizationId,
      tankId: args.tankId,
      operationalDay: prior.operational_day,
      measurement: {
        tank_id: args.tankId,
        current_measurement: prior.current_measurement,
        measured_at: prior.measured_at,
        oil_temperature_c: prior.oil_temperature_c,
        ambient_temperature_c: prior.ambient_temperature_c,
      },
      persisted: {
        gross_volume_m3: prior.gross_volume_m3,
        net_oil_volume_m3_20c: prior.net_oil_volume_m3_20c,
        volume_oil_barrels: prior.volume_oil_barrels,
        gross_volume_m3_20c: null,
        shell_temperature_c: null,
        shell_correction_factor: null,
        liquid_correction_factor: null,
        combined_correction_factor: null,
        tank_calibration_id: null,
        lab_oil_analysis_id: null,
        density_at_20c_kg_m3: null,
        water_and_sediment_percent: null,
      },
    });
  }

  const documentedOutflowGrossM3 =
    args.documentedOutflowGrossM3 ??
    (await sumTankTransferOutflowGrossM3({
      organizationId: args.organizationId,
      tankId: args.tankId,
      untilMeasuredAt: args.measuredAt,
      afterMeasuredAt: prior?.measured_at ?? null,
    }));

  assertInventoryChangeWithinBounds({
    priorGrossM3,
    newGrossM3,
    capacityGrossM3,
    hasCalibration,
    documentedOutflowGrossM3,
  });
}
