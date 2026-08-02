import { loadVolumeContextForTank } from "@lindaflor/core/tankage/tank-volume-context";
import { db } from "@lindaflor/db";
import { tankages } from "@lindaflor/db/schema/tankage";
import { maxCalibrationHeightMFromPoints } from "@lindaflor/shared/functions/tankage/calibration-volume";
import {
  tankageCurrentMeasurementSchema,
  tankageMeasuredAtSchema,
  type TankageMeasurementBounds,
  type TankageMeasurementContext,
} from "@lindaflor/shared/schemas/tankage/tankages";
import { ORPCError } from "@orpc/server";
import { and, asc, desc, eq, gt, lt, ne } from "drizzle-orm";

/**
 * Resolves the authoritative validation context for a tankage reading: the
 * immediately preceding reading (height + timestamp), the next reading timestamp,
 * and the maximum gauge height from the calibration table valid on the day.
 *
 * When `measuredAt` is null, only the latest reading is used as the previous
 * point (append scenario) and there is no next point.
 */
export async function loadTankageMeasurementContext(args: {
  organizationId: string;
  tankId: string;
  measuredAt: Date | null;
  operationalDay: string;
  excludeId?: string;
}): Promise<TankageMeasurementContext> {
  const { organizationId, tankId, measuredAt, operationalDay, excludeId } =
    args;

  const baseFilters = [
    eq(tankages.organization_id, organizationId),
    eq(tankages.tank_id, tankId),
  ];
  if (excludeId != null) {
    baseFilters.push(ne(tankages.id, excludeId));
  }

  const previousFilters =
    measuredAt != null
      ? [...baseFilters, lt(tankages.measured_at, measuredAt)]
      : baseFilters;

  const [previous] = await db
    .select({
      current_measurement: tankages.current_measurement,
      measured_at: tankages.measured_at,
    })
    .from(tankages)
    .where(and(...previousFilters))
    .orderBy(desc(tankages.measured_at), desc(tankages.created_at))
    .limit(1);

  let nextMeasuredAt: Date | null = null;
  if (measuredAt != null) {
    const [next] = await db
      .select({ measured_at: tankages.measured_at })
      .from(tankages)
      .where(and(...baseFilters, gt(tankages.measured_at, measuredAt)))
      .orderBy(asc(tankages.measured_at), asc(tankages.created_at))
      .limit(1);
    nextMeasuredAt = next?.measured_at ?? null;
  }

  const volumeContext = await loadVolumeContextForTank({
    organizationId,
    tankId,
    calibrationDayKey: operationalDay,
  });
  const points = volumeContext.calibrationPointsByTank.get(tankId) ?? [];

  return {
    previous_measurement: previous?.current_measurement ?? 0,
    capacity_height_m: maxCalibrationHeightMFromPoints(points),
    previous_measured_at: previous?.measured_at ?? null,
    next_measured_at: nextMeasuredAt,
  };
}

/**
 * Enforces the shared height and timestamp rules for a production tankage reading.
 * Set `allowDecrease` for transfer-originated rows, which legitimately lower the
 * level (they represent documented outflow) — the maximum-height and timestamp
 * guards still apply.
 */
export function assertTankageMeasurementWithinBounds(args: {
  context: TankageMeasurementContext;
  currentMeasurement: number;
  measuredAt: Date;
  allowDecrease?: boolean;
}): void {
  const heightBounds: TankageMeasurementBounds = {
    previous_measurement: args.allowDecrease
      ? 0
      : args.context.previous_measurement,
    capacity_height_m: args.context.capacity_height_m,
  };

  const heightResult = tankageCurrentMeasurementSchema(heightBounds).safeParse(
    args.currentMeasurement,
  );
  if (!heightResult.success) {
    throw new ORPCError("BAD_REQUEST", {
      message: heightResult.error.issues[0]?.message ?? "Altura inválida",
    });
  }

  const timeResult = tankageMeasuredAtSchema(args.context).safeParse(
    args.measuredAt,
  );
  if (!timeResult.success) {
    throw new ORPCError("BAD_REQUEST", {
      message: timeResult.error.issues[0]?.message ?? "Hora inválida",
    });
  }
}
