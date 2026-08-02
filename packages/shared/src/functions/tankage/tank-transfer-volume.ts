import type { CalibrationPoint } from "@lindaflor/shared/functions/tankage/calibration-volume";
import { grossVolumeFromHeight } from "@lindaflor/shared/functions/tankage/tank-measurement-volume";
import { correctStaticTankVolume } from "@lindaflor/shared/functions/tankage/tank-static-volume";

const VOLUME_EPSILON_M3 = 1e-6;

export type TransferLabInput = {
  id: string;
  density_at_20c: number;
  water_and_sediment_percent: number;
} | null;

export type TransferVolumeOk = {
  ok: true;
  gross_volume_before_m3: number;
  gross_volume_after_m3: number;
  gross_volume_out_m3: number;
  gross_volume_out_m3_20c: number | null;
  net_oil_volume_out_m3_20c: number | null;
  shell_temperature_c: number | null;
  shell_correction_factor: number | null;
  liquid_correction_factor: number | null;
  combined_correction_factor: number | null;
  lab_oil_analysis_id: string | null;
  density_at_20c_kg_m3: number | null;
  water_and_sediment_percent: number | null;
};

export type TransferVolumeErr = {
  ok: false;
  code: "HEIGHT_OUT_OF_TABLE" | "NO_OUTFLOW";
};

export function computeTransferVolumes(args: {
  calibrationPoints: readonly CalibrationPoint[];
  height_before_m: number;
  height_after_m: number;
  oil_temperature_c: number;
  ambient_temperature_c: number;
  lab: TransferLabInput;
}): TransferVolumeOk | TransferVolumeErr {
  const before = grossVolumeFromHeight(
    args.calibrationPoints,
    args.height_before_m,
  );
  const after = grossVolumeFromHeight(
    args.calibrationPoints,
    args.height_after_m,
  );
  if (before == null || after == null) {
    return { ok: false, code: "HEIGHT_OUT_OF_TABLE" };
  }
  const out = before - after;
  if (out <= VOLUME_EPSILON_M3) {
    return { ok: false, code: "NO_OUTFLOW" };
  }
  if (args.lab == null) {
    return {
      ok: true,
      gross_volume_before_m3: before,
      gross_volume_after_m3: after,
      gross_volume_out_m3: out,
      gross_volume_out_m3_20c: null,
      net_oil_volume_out_m3_20c: null,
      shell_temperature_c: null,
      shell_correction_factor: null,
      liquid_correction_factor: null,
      combined_correction_factor: null,
      lab_oil_analysis_id: null,
      density_at_20c_kg_m3: null,
      water_and_sediment_percent: null,
    };
  }
  const corrected = correctStaticTankVolume({
    gross_volume_m3: out,
    oil_temperature_c: args.oil_temperature_c,
    ambient_temperature_c: args.ambient_temperature_c,
    density_at_20c_kg_m3: args.lab.density_at_20c,
    water_and_sediment_percent: args.lab.water_and_sediment_percent,
  });
  return {
    ok: true,
    gross_volume_before_m3: before,
    gross_volume_after_m3: after,
    gross_volume_out_m3: out,
    gross_volume_out_m3_20c: corrected.gross_volume_m3_20c,
    net_oil_volume_out_m3_20c: corrected.net_oil_volume_m3_20c,
    shell_temperature_c: corrected.shell_temperature_c,
    shell_correction_factor: corrected.ctsh,
    liquid_correction_factor: corrected.ctl,
    combined_correction_factor: corrected.combined_correction_factor,
    lab_oil_analysis_id: args.lab.id,
    density_at_20c_kg_m3: args.lab.density_at_20c,
    water_and_sediment_percent: args.lab.water_and_sediment_percent,
  };
}
