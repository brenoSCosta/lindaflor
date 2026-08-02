import { describe, expect, it } from "bun:test";

import {
  computeDayProductionDelta,
  productionFromFirstAndLastMeasurements,
} from "@lindaflor/shared/functions/tankage/tank-day-production";
import {
  volumeAuditFromMeasurement,
  type LabAnalysisForVolume,
} from "@lindaflor/shared/functions/tankage/tank-measurement-volume";

const calibrationPoints = [
  { height_cm: 0, volume_m3: 0 },
  { height_cm: 100, volume_m3: 10 },
  { height_cm: 200, volume_m3: 20 },
];

const lab: LabAnalysisForVolume = {
  id: "018f3f2a-7b2c-7f3a-9b2c-7f3a9b2c7f3a",
  tank_id: "tank-1",
  collected_at: new Date("2026-01-01T00:00:00.000Z"),
  density_at_20c: 850,
  water_and_sediment_percent: 2,
};

describe("volumeAuditFromMeasurement", () => {
  it("returns gross only when lab is missing", () => {
    const audit = volumeAuditFromMeasurement({
      measurement: {
        tank_id: "tank-1",
        current_measurement: 1,
        measured_at: new Date("2026-06-01T12:00:00.000Z"),
        oil_temperature_c: 30,
        ambient_temperature_c: 25,
      },
      calibrationPoints,
      tankCalibrationId: "cal-1",
      labAnalyses: undefined,
    });

    expect(audit.gross_volume_m3).toBe(10);
    expect(audit.tank_calibration_id).toBe("cal-1");
    expect(audit.net_oil_volume_m3_20c).toBeNull();
    expect(audit.liquid_correction_factor).toBeNull();
  });

  it("fills full audit when lab is present", () => {
    const audit = volumeAuditFromMeasurement({
      measurement: {
        tank_id: "tank-1",
        current_measurement: 1,
        measured_at: new Date("2026-06-01T12:00:00.000Z"),
        oil_temperature_c: 30,
        ambient_temperature_c: 25,
      },
      calibrationPoints,
      tankCalibrationId: "cal-1",
      labAnalyses: [lab],
    });

    expect(audit.gross_volume_m3).toBe(10);
    expect(audit.lab_oil_analysis_id).toBe(lab.id);
    expect(audit.density_at_20c_kg_m3).toBe(850);
    expect(audit.combined_correction_factor).not.toBeNull();
    expect(audit.net_oil_volume_m3_20c).not.toBeNull();
    expect(audit.volume_oil_barrels).not.toBeNull();
  });
});

describe("productionFromFirstAndLastMeasurements with persisted volumes", () => {
  it("uses stored gross and net without recomputing from calibration", () => {
    const first = {
      tank_id: "tank-1",
      current_measurement: 1,
      measured_at: new Date("2026-06-01T00:01:00.000Z"),
      oil_temperature_c: 30,
      ambient_temperature_c: 25,
      gross_volume_m3: 100,
      net_oil_volume_m3_20c: 90,
      volume_oil_barrels: 565.68,
    };
    const last = {
      ...first,
      measured_at: new Date("2026-06-01T23:59:00.000Z"),
      gross_volume_m3: 110,
      net_oil_volume_m3_20c: 99,
      volume_oil_barrels: 622.29,
    };

    const production = productionFromFirstAndLastMeasurements({
      tankId: "tank-1",
      first,
      last,
      context: {
        calibrationPointsByTank: new Map(),
        labByTank: new Map(),
      },
    });

    expect(production).toEqual(
      computeDayProductionDelta({
        opening: {
          gross_volume_m3: 100,
          net_oil_volume_m3_20c: 90,
          volume_oil_barrels: 565.68,
        },
        closing: {
          gross_volume_m3: 110,
          net_oil_volume_m3_20c: 99,
          volume_oil_barrels: 622.29,
        },
      }),
    );
  });
});
