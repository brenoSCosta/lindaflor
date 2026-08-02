export type CalibrationEditablePoint = {
  id: string;
  height_cm: number;
  volume_m3: number;
};

export function newCalibrationPoint(
  height_cm = 1,
  volume_m3 = 0.001,
): CalibrationEditablePoint {
  return { id: crypto.randomUUID(), height_cm, volume_m3 };
}

export function toEditablePoints(
  points: { height_cm: number; volume_m3: number }[],
): CalibrationEditablePoint[] {
  return points.map((p) => newCalibrationPoint(p.height_cm, p.volume_m3));
}
