import { db } from "@lindaflor/db";
import {
  lab_oil_analyses,
  tank_calibration_points,
  tank_calibrations,
} from "@lindaflor/db/schema/tankage";
import type { DayProductionContext } from "@lindaflor/shared/functions/tankage/tank-day-production";
import { groupLabAnalysesByTank } from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { pickVigentCalibrationByTank } from "@lindaflor/shared/functions/tankage/vigent-calibration";
import { and, asc, desc, eq, gte, inArray, isNull, lte, or } from "drizzle-orm";

export type TankVolumeContext = DayProductionContext & {
  calibrationIdByTank: Map<string, string>;
};

function isValidOnDate(at: string) {
  return and(
    lte(tank_calibrations.valid_from, at),
    or(
      isNull(tank_calibrations.valid_until),
      gte(tank_calibrations.valid_until, at),
    ),
  );
}

export async function loadTankVolumeContext(args: {
  organizationId: string;
  tankIds: string[];
  calibrationAt: string;
}): Promise<TankVolumeContext> {
  const { organizationId, tankIds, calibrationAt } = args;

  const labRows = await db
    .select({
      id: lab_oil_analyses.id,
      tank_id: lab_oil_analyses.tank_id,
      collected_at: lab_oil_analyses.collected_at,
      density_at_20c: lab_oil_analyses.density_at_20c,
      water_and_sediment_percent: lab_oil_analyses.water_and_sediment_percent,
    })
    .from(lab_oil_analyses)
    .where(
      and(
        eq(lab_oil_analyses.organization_id, organizationId),
        inArray(lab_oil_analyses.tank_id, tankIds),
      ),
    )
    .orderBy(desc(lab_oil_analyses.collected_at));

  const labByTank = groupLabAnalysesByTank(labRows);

  const calibrations = await db
    .select({
      id: tank_calibrations.id,
      tank_id: tank_calibrations.tank_id,
      valid_from: tank_calibrations.valid_from,
      valid_until: tank_calibrations.valid_until,
    })
    .from(tank_calibrations)
    .where(
      and(
        eq(tank_calibrations.organization_id, organizationId),
        inArray(tank_calibrations.tank_id, tankIds),
        isValidOnDate(calibrationAt),
      ),
    )
    .orderBy(asc(tank_calibrations.tank_id), asc(tank_calibrations.valid_from));

  const calibrationIdsForPoints = calibrations.map((row) => row.id);
  const points =
    calibrationIdsForPoints.length > 0
      ? await db
          .select({
            calibration_id: tank_calibration_points.calibration_id,
            height_cm: tank_calibration_points.height_cm,
            volume_m3: tank_calibration_points.volume_m3,
          })
          .from(tank_calibration_points)
          .where(
            inArray(
              tank_calibration_points.calibration_id,
              calibrationIdsForPoints,
            ),
          )
          .orderBy(asc(tank_calibration_points.height_cm))
      : [];

  const pointsByCalibration = new Map<
    string,
    { height_cm: number; volume_m3: number }[]
  >();
  for (const point of points) {
    const list = pointsByCalibration.get(point.calibration_id) ?? [];
    list.push({
      height_cm: point.height_cm,
      volume_m3: point.volume_m3,
    });
    pointsByCalibration.set(point.calibration_id, list);
  }

  const vigentByTank = pickVigentCalibrationByTank(
    calibrations,
    calibrationAt,
    {
      hasPoints: (calibrationId) =>
        (pointsByCalibration.get(calibrationId)?.length ?? 0) > 0,
    },
  );

  const calibrationPointsByTank = new Map<
    string,
    { height_cm: number; volume_m3: number }[]
  >();
  const calibrationIdByTank = new Map<string, string>();
  for (const [tankId, calibration] of vigentByTank) {
    const list = pointsByCalibration.get(calibration.id) ?? [];
    calibrationPointsByTank.set(tankId, list);
    calibrationIdByTank.set(tankId, calibration.id);
  }

  return { calibrationPointsByTank, calibrationIdByTank, labByTank };
}

export async function loadVolumeContextForTank(args: {
  organizationId: string;
  tankId: string;
  calibrationDayKey: string;
}): Promise<TankVolumeContext> {
  return loadTankVolumeContext({
    organizationId: args.organizationId,
    tankIds: [args.tankId],
    calibrationAt: args.calibrationDayKey,
  });
}
