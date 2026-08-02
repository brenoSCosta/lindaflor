/**
 * Static tank volume correction — pure functions for reuse (tancagem, dreno, …).
 *
 * Spec: `apps/web/.../calculo-volume-tanque.md`
 * Norms: ASTM D1250-04 / API MPMS 11.1 (CTL), API MPMS 12.2.1 / ISO 7507 (CTSH),
 * ANP/Inmetro Resolução Conjunta nº 01/2013.
 *
 * Precision: IEEE-754 `number` (double). Do not round intermediate values;
 * round only at presentation.
 */

/** Reference temperature for ANP / API MPMS static tank correction (°C). */
export const REFERENCE_TEMPERATURE_C = 20;

/** Crude oil K0 constant (API MPMS 11.1). */
export const CRUDE_OIL_K0 = 613.9973;

/** Linear expansion coefficient for carbon steel shell (API 12.2 / ISO 7507). */
export const STEEL_LINEAR_EXPANSION = 0.0000112;

/** Petroleum barrel per cubic meter (API MPMS / ANP SI relation). */
export const OIL_BARRELS_PER_CUBIC_METER = 6.2898105697751;

/**
 * Efficiency factor (FE) default.
 * Shown on detail grids; does not scale CTL/CTSH volumes until product rules say so.
 */
export const DEFAULT_EFFICIENCY_FACTOR = 1;

export type TankStaticVolumeInput = {
  /** Gross observed volume from calibration table (m³). */
  gross_volume_m3: number;
  oil_temperature_c: number;
  ambient_temperature_c: number;
  /** Lab density at 20 °C (kg/m³). */
  density_at_20c_kg_m3: number;
  /** BSW / water and sediment (%). */
  water_and_sediment_percent: number;
  /**
   * Efficiency factor (FE). Defaults to {@link DEFAULT_EFFICIENCY_FACTOR}.
   * Not applied to volume results yet.
   */
  efficiency_factor?: number;
};

export type TankStaticVolumeResult = {
  shell_temperature_c: number;
  /** CTSH — shell (steel) correction factor. */
  shell_correction_factor: number;
  /** CTL — liquid correction factor. */
  liquid_correction_factor: number;
  /** CTL × CTSH (FE not included). */
  combined_correction_factor: number;
  /** Alias of {@link TankStaticVolumeResult.liquid_correction_factor}. */
  ctl: number;
  /** Alias of {@link TankStaticVolumeResult.shell_correction_factor}. */
  ctsh: number;
  /** FE — currently default 1; not applied to volumes. */
  efficiency_factor: number;
  gross_volume_m3_20c: number;
  net_oil_volume_m3_20c: number;
};

/** Shell metal temperature — API MPMS 12.2 weighted average. */
export function shellTemperatureC(
  oilTemperatureC: number,
  ambientTemperatureC: number,
): number {
  return (7 * oilTemperatureC + ambientTemperatureC) / 8;
}

/**
 * CTSH — shell (tank) volume correction factor.
 * Quadratic expansion (API 12.2 / ISO 7507).
 */
export function shellCorrectionFactor(shellTempC: number): number {
  const delta = shellTempC - REFERENCE_TEMPERATURE_C;
  return (1 + STEEL_LINEAR_EXPANSION * delta) ** 2;
}

/**
 * CTL — liquid volume correction factor
 * (ASTM D1250-04 / API MPMS 11.1). Use full density precision — no rounding.
 */
export function liquidCorrectionFactor(
  oilTemperatureC: number,
  densityAt20cKgM3: number,
): number {
  const alpha = CRUDE_OIL_K0 / densityAt20cKgM3 ** 2;
  const deltaT = oilTemperatureC - REFERENCE_TEMPERATURE_C;
  return Math.exp(-alpha * deltaT * (1 + 0.8 * alpha * deltaT));
}

/** Combined correction factor = CTL × CTSH (FE excluded). */
export function combinedCorrectionFactor(
  liquidFactor: number,
  shellFactor: number,
): number {
  return liquidFactor * shellFactor;
}

/**
 * Full static-tank memory of calculation:
 * T_shell → CTSH → CTL → CCF → V_bruto@20 → V_óleo@20.
 */
export function correctStaticTankVolume(
  input: TankStaticVolumeInput,
): TankStaticVolumeResult {
  const shellTemp = shellTemperatureC(
    input.oil_temperature_c,
    input.ambient_temperature_c,
  );
  const ctsh = shellCorrectionFactor(shellTemp);
  const ctl = liquidCorrectionFactor(
    input.oil_temperature_c,
    input.density_at_20c_kg_m3,
  );
  const combined = combinedCorrectionFactor(ctl, ctsh);
  const gross20 = input.gross_volume_m3 * combined;
  const net20 = gross20 * (1 - input.water_and_sediment_percent / 100);
  const efficiencyFactor = input.efficiency_factor ?? DEFAULT_EFFICIENCY_FACTOR;

  return {
    shell_temperature_c: shellTemp,
    shell_correction_factor: ctsh,
    liquid_correction_factor: ctl,
    combined_correction_factor: combined,
    ctl,
    ctsh,
    efficiency_factor: efficiencyFactor,
    gross_volume_m3_20c: gross20,
    net_oil_volume_m3_20c: net20,
  };
}

export function cubicMetersToOilBarrels(cubicMeters: number): number {
  return cubicMeters * OIL_BARRELS_PER_CUBIC_METER;
}
