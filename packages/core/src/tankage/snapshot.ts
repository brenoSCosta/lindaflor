import { loadTankDayProductionByTank } from "@lindaflor/core/tankage/day-production";
import { capacityGrossM3FromPoints } from "@lindaflor/core/tankage/tankage-stock-validation";
import { db } from "@lindaflor/db";
import {
  concessions,
  installations,
  lab_oil_analyses,
  measurement_equipments,
  tank_calibration_points,
  tank_calibrations,
  tankages,
  tanks,
} from "@lindaflor/db/schema/tankage";
import {
  maxCalibrationHeightMFromPoints,
  type CalibrationPoint,
} from "@lindaflor/shared/functions/tankage/calibration-volume";
import {
  groupLabAnalysesByTank,
  measurementVolumesFromPersisted,
  volumesFromMeasurement,
} from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import {
  calibrationVigentStatus,
  pickVigentCalibrationByTank,
} from "@lindaflor/shared/functions/tankage/vigent-calibration";
import { and, asc, desc, eq, gte, inArray, isNull, lte, or } from "drizzle-orm";

const tankBaseColumns = {
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

export type TankCalibrationSnapshotStatus =
  | "current"
  | "expired"
  | "future"
  | "none";

export type TankSnapshotRow = {
  id: string;
  tag: string;
  concession_id: string;
  concession_name: string;
  installation_id: string;
  installation_name: string;
  measurement_equipment_id: string | null;
  measurement_equipment_code: string | null;
  latitude: number | null;
  longitude: number | null;
  organization_id: string;
  created_by_user_id: string;
  created_at: Date;
  updated_at: Date;
  capacity_volume_m3: number | null;
  capacity_height_m: number | null;
  current_height_m: number | null;
  current_volume_m3: number | null;
  current_net_oil_volume_m3_20c: number | null;
  current_volume_oil_barrels: number | null;
  today_production_gross_volume_m3: number | null;
  today_production_net_oil_volume_m3_20c: number | null;
  today_production_volume_oil_barrels: number | null;
  calibration_status: TankCalibrationSnapshotStatus;
  calibration_certificate_number: string | null;
  calibration_valid_until: string | null;
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

function calibrationStatusForRow(
  validFrom: string,
  validUntil: string | null,
  today: string,
): TankCalibrationSnapshotStatus {
  const status = calibrationVigentStatus(validFrom, validUntil, today);
  return status;
}

export async function loadTankSnapshots(args: {
  organizationId: string;
  tankIds?: string[];
  at?: string;
  productionOperationalDay?: string;
}): Promise<TankSnapshotRow[]> {
  const today = args.at ?? new Date().toISOString().slice(0, 10);
  const tankFilter = and(
    eq(tanks.organization_id, args.organizationId),
    args.tankIds != null && args.tankIds.length > 0
      ? inArray(tanks.id, args.tankIds)
      : undefined,
  );

  const tankRows = await db
    .select(tankBaseColumns)
    .from(tanks)
    .innerJoin(concessions, eq(tanks.concession_id, concessions.id))
    .innerJoin(installations, eq(tanks.installation_id, installations.id))
    .leftJoin(
      measurement_equipments,
      eq(tanks.measurement_equipment_id, measurement_equipments.id),
    )
    .where(tankFilter)
    .orderBy(asc(tanks.tag));

  if (tankRows.length === 0) {
    return [];
  }

  const ids = tankRows.map((t) => t.id);

  const latestMeasurements = await db
    .selectDistinctOn([tankages.tank_id], {
      tank_id: tankages.tank_id,
      current_measurement: tankages.current_measurement,
      measured_at: tankages.measured_at,
      oil_temperature_c: tankages.oil_temperature_c,
      ambient_temperature_c: tankages.ambient_temperature_c,
      gross_volume_m3: tankages.gross_volume_m3,
      net_oil_volume_m3_20c: tankages.net_oil_volume_m3_20c,
      volume_oil_barrels: tankages.volume_oil_barrels,
      tank_calibration_id: tankages.tank_calibration_id,
    })
    .from(tankages)
    .where(
      and(
        eq(tankages.organization_id, args.organizationId),
        inArray(tankages.tank_id, ids),
      ),
    )
    .orderBy(
      tankages.tank_id,
      desc(tankages.measured_at),
      desc(tankages.created_at),
    );

  const measurementByTank = new Map(
    latestMeasurements.map((row) => [row.tank_id, row]),
  );

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
        eq(lab_oil_analyses.organization_id, args.organizationId),
        inArray(lab_oil_analyses.tank_id, ids),
      ),
    )
    .orderBy(desc(lab_oil_analyses.collected_at));

  const labByTank = groupLabAnalysesByTank(labRows);

  const currentCalibrations = await db
    .select({
      id: tank_calibrations.id,
      tank_id: tank_calibrations.tank_id,
      certificate_number: tank_calibrations.certificate_number,
      valid_from: tank_calibrations.valid_from,
      valid_until: tank_calibrations.valid_until,
    })
    .from(tank_calibrations)
    .where(
      and(
        eq(tank_calibrations.organization_id, args.organizationId),
        inArray(tank_calibrations.tank_id, ids),
        isValidOnDate(today),
      ),
    )
    .orderBy(asc(tank_calibrations.tank_id), asc(tank_calibrations.valid_from));

  const calibrationByTankForStatus = pickVigentCalibrationByTank(
    currentCalibrations,
    today,
  );

  const measurementCalibrationIds = latestMeasurements
    .map((row) => row.tank_calibration_id)
    .filter((id): id is string => id != null);

  const calibrationIdsForPoints = [
    ...new Set([
      ...currentCalibrations.map((c) => c.id),
      ...measurementCalibrationIds,
    ]),
  ];
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

  const hasCalibrationPoints = (calibrationId: string) =>
    (pointsByCalibration.get(calibrationId)?.length ?? 0) > 0;

  const calibrationByTankForTable = pickVigentCalibrationByTank(
    currentCalibrations,
    today,
    { hasPoints: hasCalibrationPoints },
  );

  const productionByTank =
    args.productionOperationalDay != null
      ? await loadTankDayProductionByTank({
          organizationId: args.organizationId,
          tankIds: ids,
          operationalDay: args.productionOperationalDay,
        })
      : null;

  return tankRows.map((tank) => {
    const measurement = measurementByTank.get(tank.id);
    const height = measurement?.current_measurement ?? null;
    const calibration = calibrationByTankForStatus.get(tank.id);
    let calibrationPoints: CalibrationPoint[] = [];

    const tableCalibration = calibrationByTankForTable.get(tank.id);
    if (tableCalibration != null) {
      calibrationPoints = pointsByCalibration.get(tableCalibration.id) ?? [];
    }
    if (
      calibrationPoints.length === 0 &&
      measurement?.tank_calibration_id != null
    ) {
      calibrationPoints =
        pointsByCalibration.get(measurement.tank_calibration_id) ?? [];
    }

    let capacity: number | null = null;
    let capacityHeightM: number | null = null;
    let currentVolume: number | null = null;
    let currentNetOilM3_20c: number | null = null;
    let currentVolumeOilBarrels: number | null = null;
    let calibrationStatus: TankCalibrationSnapshotStatus = "none";

    if (calibration) {
      calibrationStatus = calibrationStatusForRow(
        calibration.valid_from,
        calibration.valid_until,
        today,
      );
    }

    if (calibrationPoints.length > 0) {
      capacity = capacityGrossM3FromPoints(calibrationPoints);
      capacityHeightM = maxCalibrationHeightMFromPoints(calibrationPoints);
    }

    if (height != null && measurement != null && calibrationPoints.length > 0) {
      const persisted = measurementVolumesFromPersisted(measurement);
      const volumes =
        persisted ??
        volumesFromMeasurement({
          measurement: { ...measurement, tank_id: tank.id },
          calibrationPoints,
          labAnalyses: labByTank.get(tank.id),
        });
      if (volumes != null) {
        currentVolume = volumes.gross_volume_m3;
        currentNetOilM3_20c = volumes.net_oil_volume_m3_20c;
        currentVolumeOilBarrels = volumes.volume_oil_barrels;
      }
    }

    const production = productionByTank?.get(tank.id) ?? null;

    return {
      id: tank.id,
      tag: tank.tag,
      concession_id: tank.concession_id,
      concession_name: tank.concession_name,
      installation_id: tank.installation_id,
      installation_name: tank.installation_name,
      measurement_equipment_id: tank.measurement_equipment_id,
      measurement_equipment_code: tank.measurement_equipment_code,
      latitude: tank.latitude,
      longitude: tank.longitude,
      organization_id: tank.organization_id,
      created_by_user_id: tank.created_by_user_id,
      created_at: tank.created_at,
      updated_at: tank.updated_at,
      capacity_volume_m3: capacity,
      capacity_height_m: capacityHeightM,
      current_height_m: height,
      current_volume_m3: currentVolume,
      current_net_oil_volume_m3_20c: currentNetOilM3_20c,
      current_volume_oil_barrels: currentVolumeOilBarrels,
      today_production_gross_volume_m3:
        production?.production_gross_volume_m3 ?? null,
      today_production_net_oil_volume_m3_20c:
        production?.production_net_oil_volume_m3_20c ?? null,
      today_production_volume_oil_barrels:
        production?.production_volume_oil_barrels ?? null,
      calibration_status: calibrationStatus,
      calibration_certificate_number: calibration?.certificate_number ?? null,
      calibration_valid_until: calibration?.valid_until ?? null,
    };
  });
}
