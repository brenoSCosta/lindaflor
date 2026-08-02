import type { CalibrationEditablePoint } from "@/routes/(auth)/arqueacao/-components/calibration-points";

export type HeightBounds = { min: number; max: number };

export type HeightBand = {
  from: number;
  to: number;
  label: string;
};

export const HEIGHT_BAND_STEP_CM = 30;

export const ALL_HEIGHT_BANDS_KEY = "__all__";

export function heightBounds(points: CalibrationEditablePoint[]): HeightBounds {
  if (points.length === 0) return { min: 0, max: 100 };
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const point of points) {
    min = Math.min(min, point.height_cm);
    max = Math.max(max, point.height_cm);
  }
  if (min === max) return { min, max: min + 1 };
  return { min, max };
}

function formatBandLabel(from: number, to: number): string {
  const fromText = from.toLocaleString("pt-BR", { maximumFractionDigits: 3 });
  const toText = to.toLocaleString("pt-BR", { maximumFractionDigits: 3 });
  return `${fromText} – ${toText} cm`;
}

export function heightBandKey(from: number, to: number): string {
  return `${from}:${to}`;
}

export function buildHeightBands(
  min: number,
  max: number,
  stepCm = HEIGHT_BAND_STEP_CM,
): HeightBand[] {
  if (stepCm <= 0 || min > max) return [];

  const bands: HeightBand[] = [];
  const firstK = Math.floor(min / stepCm);
  const lastK = Math.floor(max / stepCm);

  for (let k = firstK; k <= lastK; k += 1) {
    const bandStart = k * stepCm;
    const bandEnd = Math.min((k + 1) * stepCm, max);
    const from = Math.max(min, bandStart);
    const to = bandEnd;
    if (from > to) continue;
    bands.push({
      from,
      to,
      label: formatBandLabel(from, to),
    });
  }

  return bands;
}

export function resolveSelectedBandKey(
  selectedKey: string,
  bands: HeightBand[],
): string {
  if (selectedKey === ALL_HEIGHT_BANDS_KEY) return ALL_HEIGHT_BANDS_KEY;
  if (bands.some((band) => heightBandKey(band.from, band.to) === selectedKey)) {
    return selectedKey;
  }
  return ALL_HEIGHT_BANDS_KEY;
}

export function rangeForBandKey(
  selectedKey: string,
  bands: HeightBand[],
  fullRange: [number, number],
): [number, number] {
  if (selectedKey === ALL_HEIGHT_BANDS_KEY) return fullRange;
  const band = bands.find(
    (item) => heightBandKey(item.from, item.to) === selectedKey,
  );
  if (band == null) return fullRange;
  return [band.from, band.to];
}
