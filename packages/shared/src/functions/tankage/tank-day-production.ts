import type { CalibrationPoint } from "@lindaflor/shared/functions/tankage/calibration-volume";
import {
  measurementVolumesFromPersisted,
  volumesFromMeasurement,
  type LabAnalysisForVolume,
  type MeasurementVolumes,
  type MeasurementWithPersistedVolumes,
} from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { cubicMetersToOilBarrels } from "@lindaflor/shared/functions/tankage/tank-static-volume";

/** Daily production from opening/closing stock (transfer/retratamento not applied yet). */
export type TankDayProductionVolumes = {
  production_gross_volume_m3: number;
  production_net_oil_volume_m3_20c: number | null;
  production_volume_oil_barrels: number | null;
};

export function computeDayProductionDelta(args: {
  opening: MeasurementVolumes;
  closing: MeasurementVolumes;
}): TankDayProductionVolumes {
  const production_gross_volume_m3 =
    args.closing.gross_volume_m3 - args.opening.gross_volume_m3;

  if (
    args.opening.net_oil_volume_m3_20c != null &&
    args.closing.net_oil_volume_m3_20c != null
  ) {
    const production_net_oil_volume_m3_20c =
      args.closing.net_oil_volume_m3_20c - args.opening.net_oil_volume_m3_20c;
    return {
      production_gross_volume_m3,
      production_net_oil_volume_m3_20c,
      production_volume_oil_barrels: cubicMetersToOilBarrels(
        production_net_oil_volume_m3_20c,
      ),
    };
  }

  return {
    production_gross_volume_m3,
    production_net_oil_volume_m3_20c: null,
    production_volume_oil_barrels: null,
  };
}

export type DayProductionContext = {
  calibrationPointsByTank: Map<string, readonly CalibrationPoint[]>;
  labByTank: Map<string, LabAnalysisForVolume[]>;
};

function resolveMeasurementVolumes(
  tankId: string,
  measurement: MeasurementWithPersistedVolumes,
  context: DayProductionContext,
): MeasurementVolumes | null {
  const persisted = measurementVolumesFromPersisted(measurement);
  if (persisted != null) {
    return persisted;
  }

  const points = context.calibrationPointsByTank.get(tankId) ?? [];
  const lab = context.labByTank.get(tankId);

  return volumesFromMeasurement({
    measurement,
    calibrationPoints: points,
    labAnalyses: lab,
  });
}

export function productionFromFirstAndLastMeasurements(args: {
  tankId: string;
  first: MeasurementWithPersistedVolumes;
  last: MeasurementWithPersistedVolumes;
  context: DayProductionContext;
}): TankDayProductionVolumes | null {
  const opening = resolveMeasurementVolumes(
    args.tankId,
    args.first,
    args.context,
  );
  const closing = resolveMeasurementVolumes(
    args.tankId,
    args.last,
    args.context,
  );

  if (opening == null || closing == null) {
    return null;
  }

  return computeDayProductionDelta({ opening, closing });
}
