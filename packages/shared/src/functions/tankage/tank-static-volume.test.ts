import { describe, expect, test } from "bun:test";

import {
  combinedCorrectionFactor,
  correctStaticTankVolume,
  cubicMetersToOilBarrels,
  DEFAULT_EFFICIENCY_FACTOR,
  liquidCorrectionFactor,
  OIL_BARRELS_PER_CUBIC_METER,
  REFERENCE_TEMPERATURE_C,
  shellCorrectionFactor,
  shellTemperatureC,
} from "@lindaflor/shared/functions/tankage/tank-static-volume";

/**
 * Golden vector from the C# reference block in
 * `calculo-volume-tanque.md` §8 (same constants and inputs).
 * Expected values computed independently with IEEE-754 double.
 */
const MD_GOLDEN = {
  oil_temperature_c: 32.5,
  ambient_temperature_c: 28,
  density_at_20c_kg_m3: 850,
  gross_volume_m3: 100,
  water_and_sediment_percent: 2.5,
  shell_temperature_c: 31.9375,
  shell_correction_factor: 1.0002674178756898,
  liquid_correction_factor: 0.9893441149097574,
  combined_correction_factor: 0.9896086832112928,
  gross_volume_m3_20c: 98.96086832112928,
  net_oil_volume_m3_20c: 96.48684661310105,
} as const;

describe("shellTemperatureC", () => {
  test("weighted average per API 12.2 (7 oil + 1 ambient) / 8", () => {
    expect(shellTemperatureC(30, 25)).toBe((7 * 30 + 25) / 8);
  });

  test("matches MD golden shell temperature", () => {
    expect(
      shellTemperatureC(
        MD_GOLDEN.oil_temperature_c,
        MD_GOLDEN.ambient_temperature_c,
      ),
    ).toBe(MD_GOLDEN.shell_temperature_c);
  });
});

describe("shellCorrectionFactor (CTSH)", () => {
  test("is 1 at reference temperature", () => {
    expect(shellCorrectionFactor(REFERENCE_TEMPERATURE_C)).toBe(1);
  });

  test("matches MD golden CTSH", () => {
    expect(shellCorrectionFactor(MD_GOLDEN.shell_temperature_c)).toBeCloseTo(
      MD_GOLDEN.shell_correction_factor,
      15,
    );
  });
});

describe("liquidCorrectionFactor (CTL)", () => {
  test("is 1 at reference temperature (any positive density)", () => {
    expect(liquidCorrectionFactor(REFERENCE_TEMPERATURE_C, 850)).toBeCloseTo(
      1,
      15,
    );
  });

  test("matches MD golden CTL", () => {
    expect(
      liquidCorrectionFactor(
        MD_GOLDEN.oil_temperature_c,
        MD_GOLDEN.density_at_20c_kg_m3,
      ),
    ).toBeCloseTo(MD_GOLDEN.liquid_correction_factor, 15);
  });
});

describe("combinedCorrectionFactor", () => {
  test("is CTL × CTSH (FE not applied)", () => {
    const ctl = MD_GOLDEN.liquid_correction_factor;
    const ctsh = MD_GOLDEN.shell_correction_factor;
    expect(combinedCorrectionFactor(ctl, ctsh)).toBeCloseTo(ctl * ctsh, 15);
  });
});

describe("correctStaticTankVolume", () => {
  test("matches C# reference block from calculo-volume-tanque.md §8", () => {
    const result = correctStaticTankVolume({
      gross_volume_m3: MD_GOLDEN.gross_volume_m3,
      oil_temperature_c: MD_GOLDEN.oil_temperature_c,
      ambient_temperature_c: MD_GOLDEN.ambient_temperature_c,
      density_at_20c_kg_m3: MD_GOLDEN.density_at_20c_kg_m3,
      water_and_sediment_percent: MD_GOLDEN.water_and_sediment_percent,
    });

    expect(result.shell_temperature_c).toBe(MD_GOLDEN.shell_temperature_c);
    expect(result.shell_correction_factor).toBeCloseTo(
      MD_GOLDEN.shell_correction_factor,
      15,
    );
    expect(result.liquid_correction_factor).toBeCloseTo(
      MD_GOLDEN.liquid_correction_factor,
      15,
    );
    expect(result.combined_correction_factor).toBeCloseTo(
      MD_GOLDEN.combined_correction_factor,
      15,
    );
    expect(result.gross_volume_m3_20c).toBeCloseTo(
      MD_GOLDEN.gross_volume_m3_20c,
      15,
    );
    expect(result.net_oil_volume_m3_20c).toBeCloseTo(
      MD_GOLDEN.net_oil_volume_m3_20c,
      15,
    );
  });

  test("exposes CTL/CTSH aliases equal to liquid/shell factors", () => {
    const result = correctStaticTankVolume({
      gross_volume_m3: MD_GOLDEN.gross_volume_m3,
      oil_temperature_c: MD_GOLDEN.oil_temperature_c,
      ambient_temperature_c: MD_GOLDEN.ambient_temperature_c,
      density_at_20c_kg_m3: MD_GOLDEN.density_at_20c_kg_m3,
      water_and_sediment_percent: MD_GOLDEN.water_and_sediment_percent,
    });

    expect(result.ctl).toBe(result.liquid_correction_factor);
    expect(result.ctsh).toBe(result.shell_correction_factor);
  });

  test("defaults FE to 1 and does not scale volumes by FE yet", () => {
    const result = correctStaticTankVolume({
      gross_volume_m3: MD_GOLDEN.gross_volume_m3,
      oil_temperature_c: MD_GOLDEN.oil_temperature_c,
      ambient_temperature_c: MD_GOLDEN.ambient_temperature_c,
      density_at_20c_kg_m3: MD_GOLDEN.density_at_20c_kg_m3,
      water_and_sediment_percent: MD_GOLDEN.water_and_sediment_percent,
    });

    expect(DEFAULT_EFFICIENCY_FACTOR).toBe(1);
    expect(result.efficiency_factor).toBe(DEFAULT_EFFICIENCY_FACTOR);
    expect(result.gross_volume_m3_20c).toBeCloseTo(
      MD_GOLDEN.gross_volume_m3 * result.combined_correction_factor,
      15,
    );
  });

  test("volume chain: bruto@20 = amb × CTL × CTSH; óleo@20 = bruto@20 × (1 − BSW/100)", () => {
    const grossAmb = 2.63;
    const bswPercent = 0.71;
    const result = correctStaticTankVolume({
      gross_volume_m3: grossAmb,
      oil_temperature_c: MD_GOLDEN.oil_temperature_c,
      ambient_temperature_c: MD_GOLDEN.ambient_temperature_c,
      density_at_20c_kg_m3: 903.8,
      water_and_sediment_percent: bswPercent,
    });

    expect(result.gross_volume_m3_20c).toBeCloseTo(
      grossAmb * result.ctl * result.ctsh,
      15,
    );
    expect(result.net_oil_volume_m3_20c).toBeCloseTo(
      result.gross_volume_m3_20c * (1 - bswPercent / 100),
      15,
    );
  });
});

/**
 * Legacy "Detalhamento" row (sistema base) — dens shown as 0,90380 (relativa),
 * BSW 0,71000, volumes/fatores a 5 casas. Temps na UI apareciam 0,000;
 * oil/ambient abaixo foram invertidos de CTL/CTSH com ρ₂₀ = 903,8 kg/m³
 * para reproduzir os fatores exibidos.
 */
const LEGACY_DETALHAMENTO_GOLDEN = {
  gross_volume_m3: 2.63,
  density_at_20c_kg_m3: 903.8,
  water_and_sediment_percent: 0.71,
  oil_temperature_c: 0.04466933354908065,
  ambient_temperature_c: -1.045055475488624,
  displayed: {
    ctl: 1.01493,
    ctsh: 0.99955,
    gross_volume_m3_20c: 2.66808,
    net_oil_volume_m3_20c: 2.64914,
  },
} as const;

function round5(value: number): number {
  return Number(value.toFixed(5));
}

describe("legacy detalhamento golden (sistema base)", () => {
  test("CTL/CTSH match displayed 5-decimal factors", () => {
    const result = correctStaticTankVolume({
      gross_volume_m3: LEGACY_DETALHAMENTO_GOLDEN.gross_volume_m3,
      oil_temperature_c: LEGACY_DETALHAMENTO_GOLDEN.oil_temperature_c,
      ambient_temperature_c: LEGACY_DETALHAMENTO_GOLDEN.ambient_temperature_c,
      density_at_20c_kg_m3: LEGACY_DETALHAMENTO_GOLDEN.density_at_20c_kg_m3,
      water_and_sediment_percent:
        LEGACY_DETALHAMENTO_GOLDEN.water_and_sediment_percent,
    });

    expect(round5(result.ctl)).toBe(LEGACY_DETALHAMENTO_GOLDEN.displayed.ctl);
    expect(round5(result.ctsh)).toBe(LEGACY_DETALHAMENTO_GOLDEN.displayed.ctsh);
  });

  test("volumes match factor chain; display differs by legacy rounding (~2e-5)", () => {
    const result = correctStaticTankVolume({
      gross_volume_m3: LEGACY_DETALHAMENTO_GOLDEN.gross_volume_m3,
      oil_temperature_c: LEGACY_DETALHAMENTO_GOLDEN.oil_temperature_c,
      ambient_temperature_c: LEGACY_DETALHAMENTO_GOLDEN.ambient_temperature_c,
      density_at_20c_kg_m3: LEGACY_DETALHAMENTO_GOLDEN.density_at_20c_kg_m3,
      water_and_sediment_percent:
        LEGACY_DETALHAMENTO_GOLDEN.water_and_sediment_percent,
    });

    const chainedGross20 =
      LEGACY_DETALHAMENTO_GOLDEN.gross_volume_m3 *
      LEGACY_DETALHAMENTO_GOLDEN.displayed.ctl *
      LEGACY_DETALHAMENTO_GOLDEN.displayed.ctsh;
    const chainedNet20 =
      chainedGross20 *
      (1 - LEGACY_DETALHAMENTO_GOLDEN.water_and_sediment_percent / 100);

    expect(result.gross_volume_m3_20c).toBeCloseTo(chainedGross20, 12);
    expect(result.net_oil_volume_m3_20c).toBeCloseTo(chainedNet20, 12);
    expect(result.gross_volume_m3_20c).toBeCloseTo(
      LEGACY_DETALHAMENTO_GOLDEN.displayed.gross_volume_m3_20c,
      4,
    );
    expect(result.net_oil_volume_m3_20c).toBeCloseTo(
      LEGACY_DETALHAMENTO_GOLDEN.displayed.net_oil_volume_m3_20c,
      4,
    );
  });
});

describe("cubicMetersToOilBarrels", () => {
  test("uses ANP SI factor", () => {
    expect(cubicMetersToOilBarrels(1)).toBe(OIL_BARRELS_PER_CUBIC_METER);
  });
});
