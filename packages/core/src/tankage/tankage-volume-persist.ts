import { loadVolumeContextForTank } from "@lindaflor/core/tankage/tank-volume-context";
import { db } from "@lindaflor/db";
import { tankages } from "@lindaflor/db/schema/tankage";
import {
  volumeAuditFromMeasurement,
  type MeasurementForVolume,
  type MeasurementVolumePersisted,
} from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { eq } from "drizzle-orm";

export type TankageVolumeInsertColumns = MeasurementVolumePersisted;

export function volumeColumnsFromAudit(
  audit: MeasurementVolumePersisted,
): TankageVolumeInsertColumns {
  return audit;
}

export async function computeTankageVolumeColumns(args: {
  organizationId: string;
  tankId: string;
  operationalDay: string;
  measurement: MeasurementForVolume;
}): Promise<TankageVolumeInsertColumns> {
  const context = await loadVolumeContextForTank({
    organizationId: args.organizationId,
    tankId: args.tankId,
    calibrationDayKey: args.operationalDay,
  });

  const points = context.calibrationPointsByTank.get(args.tankId) ?? [];
  const calibrationId = context.calibrationIdByTank.get(args.tankId) ?? null;
  const lab = context.labByTank.get(args.tankId);

  return volumeAuditFromMeasurement({
    measurement: args.measurement,
    calibrationPoints: points,
    tankCalibrationId: calibrationId,
    labAnalyses: lab,
  });
}

export async function recomputeAllTankageVolumesForOrganization(
  organizationId: string,
): Promise<number> {
  const rows = await db
    .select({
      id: tankages.id,
      tank_id: tankages.tank_id,
      operational_day: tankages.operational_day,
      current_measurement: tankages.current_measurement,
      measured_at: tankages.measured_at,
      oil_temperature_c: tankages.oil_temperature_c,
      ambient_temperature_c: tankages.ambient_temperature_c,
    })
    .from(tankages)
    .where(eq(tankages.organization_id, organizationId));

  let updated = 0;
  await Promise.all(
    rows.map(async (row) => {
      const volumes = await computeTankageVolumeColumns({
        organizationId,
        tankId: row.tank_id,
        operationalDay: row.operational_day,
        measurement: {
          tank_id: row.tank_id,
          current_measurement: row.current_measurement,
          measured_at: row.measured_at,
          oil_temperature_c: row.oil_temperature_c,
          ambient_temperature_c: row.ambient_temperature_c,
        },
      });

      await db.update(tankages).set(volumes).where(eq(tankages.id, row.id));
      updated += 1;
    }),
  );

  return updated;
}
