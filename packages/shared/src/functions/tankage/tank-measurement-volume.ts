import type { CalibrationPoint } from "@lindaflor/shared/functions/tankage/calibration-volume";
import { interpolateVolume } from "@lindaflor/shared/functions/tankage/calibration-volume";
import {
  correctStaticTankVolume,
  cubicMetersToOilBarrels,
} from "@lindaflor/shared/functions/tankage/tank-static-volume";

export type LabAnalysisForVolume = {
  id: string;
  tank_id: string;
  collected_at: Date;
  density_at_20c: number;
  water_and_sediment_percent: number;
};

export type MeasurementForVolume = {
  tank_id: string;
  current_measurement: number;
  measured_at: Date;
  oil_temperature_c: number;
  ambient_temperature_c: number;
};

export type MeasurementVolumes = {
  gross_volume_m3: number;
  net_oil_volume_m3_20c: number | null;
  volume_oil_barrels: number | null;
};

export type PersistedMeasurementVolumes = {
  gross_volume_m3: number | null;
  net_oil_volume_m3_20c: number | null;
  volume_oil_barrels: number | null;
};

export type MeasurementWithPersistedVolumes = MeasurementForVolume &
  PersistedMeasurementVolumes;

export type MeasurementVolumePersisted = {
  gross_volume_m3: number | null;
  gross_volume_m3_20c: number | null;
  net_oil_volume_m3_20c: number | null;
  volume_oil_barrels: number | null;
  shell_temperature_c: number | null;
  shell_correction_factor: number | null;
  liquid_correction_factor: number | null;
  combined_correction_factor: number | null;
  tank_calibration_id: string | null;
  lab_oil_analysis_id: string | null;
  density_at_20c_kg_m3: number | null;
  water_and_sediment_percent: number | null;
};

const emptyVolumePersisted = (): MeasurementVolumePersisted => ({
  gross_volume_m3: null,
  gross_volume_m3_20c: null,
  net_oil_volume_m3_20c: null,
  volume_oil_barrels: null,
  shell_temperature_c: null,
  shell_correction_factor: null,
  liquid_correction_factor: null,
  combined_correction_factor: null,
  tank_calibration_id: null,
  lab_oil_analysis_id: null,
  density_at_20c_kg_m3: null,
  water_and_sediment_percent: null,
});

export function groupLabAnalysesByTank(
  rows: LabAnalysisForVolume[],
): Map<string, LabAnalysisForVolume[]> {
  const byTank = new Map<string, LabAnalysisForVolume[]>();
  for (const row of rows) {
    const list = byTank.get(row.tank_id) ?? [];
    list.push(row);
    byTank.set(row.tank_id, list);
  }
  for (const list of byTank.values()) {
    list.sort((a, b) => b.collected_at.getTime() - a.collected_at.getTime());
  }
  return byTank;
}

export function resolveLabAnalysis(
  analyses: LabAnalysisForVolume[] | undefined,
  measuredAt: Date,
): LabAnalysisForVolume | null {
  if (analyses == null || analyses.length === 0) {
    return null;
  }
  const atOrBefore = analyses.find(
    (row) => row.collected_at.getTime() <= measuredAt.getTime(),
  );
  return atOrBefore ?? analyses[0] ?? null;
}

export function grossVolumeFromHeight(
  calibrationPoints: readonly CalibrationPoint[],
  heightM: number,
): number | null {
  if (calibrationPoints.length === 0) {
    return null;
  }
  const result = interpolateVolume(calibrationPoints, heightM * 100);
  return result.ok ? result.volume_m3 : null;
}

export function measurementVolumesFromPersisted(
  row: PersistedMeasurementVolumes,
): MeasurementVolumes | null {
  if (row.gross_volume_m3 == null) {
    return null;
  }
  return {
    gross_volume_m3: row.gross_volume_m3,
    net_oil_volume_m3_20c: row.net_oil_volume_m3_20c,
    volume_oil_barrels: row.volume_oil_barrels,
  };
}

export function volumeAuditFromMeasurement(args: {
  measurement: MeasurementForVolume;
  calibrationPoints: readonly CalibrationPoint[];
  tankCalibrationId: string | null;
  labAnalyses: LabAnalysisForVolume[] | undefined;
}): MeasurementVolumePersisted {
  const gross = grossVolumeFromHeight(
    args.calibrationPoints,
    args.measurement.current_measurement,
  );
  if (gross == null) {
    return emptyVolumePersisted();
  }

  const base: MeasurementVolumePersisted = {
    ...emptyVolumePersisted(),
    gross_volume_m3: gross,
    tank_calibration_id: args.tankCalibrationId,
  };

  const lab = resolveLabAnalysis(
    args.labAnalyses,
    args.measurement.measured_at,
  );
  if (lab == null) {
    return base;
  }

  const corrected = correctStaticTankVolume({
    gross_volume_m3: gross,
    oil_temperature_c: args.measurement.oil_temperature_c,
    ambient_temperature_c: args.measurement.ambient_temperature_c,
    density_at_20c_kg_m3: lab.density_at_20c,
    water_and_sediment_percent: lab.water_and_sediment_percent,
  });
  const net = corrected.net_oil_volume_m3_20c;

  return {
    gross_volume_m3: gross,
    gross_volume_m3_20c: corrected.gross_volume_m3_20c,
    net_oil_volume_m3_20c: net,
    volume_oil_barrels: cubicMetersToOilBarrels(net),
    shell_temperature_c: corrected.shell_temperature_c,
    shell_correction_factor: corrected.shell_correction_factor,
    liquid_correction_factor: corrected.liquid_correction_factor,
    combined_correction_factor: corrected.combined_correction_factor,
    tank_calibration_id: args.tankCalibrationId,
    lab_oil_analysis_id: lab.id,
    density_at_20c_kg_m3: lab.density_at_20c,
    water_and_sediment_percent: lab.water_and_sediment_percent,
  };
}

export function volumesFromMeasurement(args: {
  measurement: MeasurementForVolume;
  calibrationPoints: readonly CalibrationPoint[];
  labAnalyses: LabAnalysisForVolume[] | undefined;
}): MeasurementVolumes | null {
  const audit = volumeAuditFromMeasurement({
    measurement: args.measurement,
    calibrationPoints: args.calibrationPoints,
    tankCalibrationId: null,
    labAnalyses: args.labAnalyses,
  });
  if (audit.gross_volume_m3 == null) {
    return null;
  }
  return {
    gross_volume_m3: audit.gross_volume_m3,
    net_oil_volume_m3_20c: audit.net_oil_volume_m3_20c,
    volume_oil_barrels: audit.volume_oil_barrels,
  };
}
