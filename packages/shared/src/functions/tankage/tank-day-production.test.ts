import { describe, expect, test } from "bun:test";

import { computeDayProductionDelta } from "@lindaflor/shared/functions/tankage/tank-day-production";
import { cubicMetersToOilBarrels } from "@lindaflor/shared/functions/tankage/tank-static-volume";

describe("computeDayProductionDelta", () => {
  test("subtracts gross and net when lab volumes exist", () => {
    const opening = {
      gross_volume_m3: 100,
      net_oil_volume_m3_20c: 95,
      volume_oil_barrels: cubicMetersToOilBarrels(95),
    };
    const closing = {
      gross_volume_m3: 110,
      net_oil_volume_m3_20c: 104,
      volume_oil_barrels: cubicMetersToOilBarrels(104),
    };

    const result = computeDayProductionDelta({ opening, closing });
    expect(result.production_gross_volume_m3).toBeCloseTo(10, 12);
    expect(result.production_net_oil_volume_m3_20c).toBeCloseTo(9, 12);
    expect(result.production_volume_oil_barrels).toBeCloseTo(
      cubicMetersToOilBarrels(9),
      10,
    );
  });

  test("returns gross only when net is missing on either side", () => {
    const opening = {
      gross_volume_m3: 50,
      net_oil_volume_m3_20c: null,
      volume_oil_barrels: null,
    };
    const closing = {
      gross_volume_m3: 55,
      net_oil_volume_m3_20c: 52,
      volume_oil_barrels: cubicMetersToOilBarrels(52),
    };

    const result = computeDayProductionDelta({ opening, closing });
    expect(result.production_gross_volume_m3).toBeCloseTo(5, 12);
    expect(result.production_net_oil_volume_m3_20c).toBeNull();
    expect(result.production_volume_oil_barrels).toBeNull();
  });
});
