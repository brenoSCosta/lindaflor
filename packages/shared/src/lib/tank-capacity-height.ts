import { maxCalibrationHeightMFromPoints } from "@lindaflor/shared/functions/tankage/calibration-volume";
import {
  pickVigentCalibrationByTank,
  type CalibrationValidityRow,
} from "@lindaflor/shared/functions/tankage/vigent-calibration";

/**
 * Maximum gauge height (m) for a tank on a calendar day, from the vigente
 * calibration certificate that has points. Pure — safe for native and web.
 */
export function capacityHeightMForTank(args: {
  calibrations: readonly CalibrationValidityRow[];
  points: readonly {
    calibration_id: string;
    height_cm: number;
    volume_m3?: number;
  }[];
  tankId: string;
  onDate: string;
}): number | null {
  const tankCalibrations = args.calibrations.filter(
    (row) => row.tank_id === args.tankId,
  );
  if (tankCalibrations.length === 0) {
    return null;
  }

  const pointsByCalibration = new Map<
    string,
    { height_cm: number; volume_m3: number }[]
  >();
  for (const point of args.points) {
    const list = pointsByCalibration.get(point.calibration_id) ?? [];
    list.push({
      height_cm: point.height_cm,
      volume_m3: point.volume_m3 ?? 0,
    });
    pointsByCalibration.set(point.calibration_id, list);
  }

  const vigentByTank = pickVigentCalibrationByTank(
    tankCalibrations,
    args.onDate,
    {
      hasPoints: (calibrationId) =>
        (pointsByCalibration.get(calibrationId)?.length ?? 0) > 0,
    },
  );
  const vigent = vigentByTank.get(args.tankId);
  if (vigent == null) {
    return null;
  }

  const points = pointsByCalibration.get(vigent.id) ?? [];
  return maxCalibrationHeightMFromPoints(points);
}
