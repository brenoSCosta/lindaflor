import { describe, expect, test } from "bun:test";

import { correctStaticTankVolume } from "@lindaflor/shared/functions/tankage/tank-static-volume";
import { computeTransferVolumes } from "@lindaflor/shared/functions/tankage/tank-transfer-volume";

const points = [
  { height_cm: 0, volume_m3: 0 },
  { height_cm: 100, volume_m3: 10 },
  { height_cm: 200, volume_m3: 20 },
];

describe("computeTransferVolumes", () => {
  test("computes ambient delta and 20c when lab present", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 1.5,
      height_after_m: 1.0,
      oil_temperature_c: 32.5,
      ambient_temperature_c: 28,
      lab: {
        id: "018f0000-0000-7000-8000-000000000001",
        density_at_20c: 850,
        water_and_sediment_percent: 2.5,
      },
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.gross_volume_before_m3).toBe(15);
    expect(result.gross_volume_after_m3).toBe(10);
    expect(result.gross_volume_out_m3).toBe(5);
    const expected = correctStaticTankVolume({
      gross_volume_m3: 5,
      oil_temperature_c: 32.5,
      ambient_temperature_c: 28,
      density_at_20c_kg_m3: 850,
      water_and_sediment_percent: 2.5,
    });
    expect(result.gross_volume_out_m3_20c).toBeCloseTo(
      expected.gross_volume_m3_20c,
      12,
    );
    expect(result.net_oil_volume_out_m3_20c).toBeCloseTo(
      expected.net_oil_volume_m3_20c,
      12,
    );
    expect(result.liquid_correction_factor).toBeCloseTo(expected.ctl, 12);
  });

  test("returns ambient-only when lab missing", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 1.5,
      height_after_m: 1.0,
      oil_temperature_c: 30,
      ambient_temperature_c: 25,
      lab: null,
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.gross_volume_out_m3).toBe(5);
    expect(result.gross_volume_out_m3_20c).toBeNull();
    expect(result.lab_oil_analysis_id).toBeNull();
  });

  test("fails when after height is not lower", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 1.0,
      height_after_m: 1.0,
      oil_temperature_c: 30,
      ambient_temperature_c: 25,
      lab: null,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("NO_OUTFLOW");
  });

  test("fails when height outside calibration", () => {
    const result = computeTransferVolumes({
      calibrationPoints: points,
      height_before_m: 3.0,
      height_after_m: 1.0,
      oil_temperature_c: 30,
      ambient_temperature_c: 25,
      lab: null,
    });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe("HEIGHT_OUT_OF_TABLE");
  });
});
