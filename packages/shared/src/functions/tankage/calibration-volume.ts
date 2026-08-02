/**
 * Linear interpolation of volume from a tank calibration (arqueação) table.
 * Heights are in centimetres (por cm), sorted ascending.
 * Height outside the table range is an error.
 */
export type CalibrationPoint = {
  height_cm: number;
  volume_m3: number;
};

export type InterpolateVolumeResult =
  | { ok: true; volume_m3: number; interpolated: boolean }
  | { ok: false; message: string };

/** Sentinel used only for overlap math when valid_until is open-ended. */
export const OPEN_ENDED_UNTIL = "9999-12-31";

export function effectiveValidUntil(validUntil: string | null): string {
  return validUntil ?? OPEN_ENDED_UNTIL;
}

export function interpolateVolume(
  points: readonly CalibrationPoint[],
  heightCm: number,
): InterpolateVolumeResult {
  if (points.length === 0) {
    return {
      ok: false,
      message: "Tabela de arqueação sem pontos cadastrados",
    };
  }

  const first = points[0];
  const last = points[points.length - 1];
  if (first == null || last == null) {
    return {
      ok: false,
      message: "Tabela de arqueação sem pontos cadastrados",
    };
  }

  if (heightCm < first.height_cm || heightCm > last.height_cm) {
    return {
      ok: false,
      message: `Altura ${heightCm} cm fora da tabela de arqueação (${first.height_cm}-${last.height_cm} cm)`,
    };
  }

  for (let i = 0; i < points.length; i++) {
    const point = points[i];
    if (point == null) continue;
    if (point.height_cm === heightCm) {
      return { ok: true, volume_m3: point.volume_m3, interpolated: false };
    }
    const next = points[i + 1];
    if (next == null) break;
    if (heightCm > point.height_cm && heightCm < next.height_cm) {
      const span = next.height_cm - point.height_cm;
      const t = (heightCm - point.height_cm) / span;
      const volume_m3 =
        point.volume_m3 + t * (next.volume_m3 - point.volume_m3);
      return { ok: true, volume_m3, interpolated: true };
    }
  }

  return {
    ok: false,
    message: `Não foi possível interpolar volume para altura ${heightCm} cm`,
  };
}

/** Maximum gauge height from calibration table (metres). */
export function maxCalibrationHeightMFromPoints(
  points: readonly CalibrationPoint[],
): number | null {
  if (points.length === 0) {
    return null;
  }
  const maxCm = Math.max(...points.map((point) => point.height_cm));
  return Number.isFinite(maxCm) ? maxCm / 100 : null;
}

/** Inclusive date ranges; null valid_until means open-ended. */
export function dateRangesOverlap(
  aFrom: string,
  aUntil: string | null,
  bFrom: string,
  bUntil: string | null,
): boolean {
  return (
    aFrom <= effectiveValidUntil(bUntil) && bFrom <= effectiveValidUntil(aUntil)
  );
}

/** Calendar day before an ISO date (yyyy-MM-dd), UTC-safe. */
export function dayBeforeIsoDate(isoDate: string): string {
  const date = new Date(`${isoDate}T12:00:00.000Z`);
  date.setUTCDate(date.getUTCDate() - 1);
  return date.toISOString().slice(0, 10);
}

export type ValidityRange = {
  id?: string;
  valid_from: string;
  valid_until: string | null;
};

/**
 * Close open-ended certificates that start before the new range so the new
 * certificate can take over from validFrom without overlapping.
 */
export function planOpenEndedClosures(
  existing: ValidityRange[],
  validFrom: string,
  validUntil: string | null,
): { closes: { id: string; valid_until: string }[]; conflict?: ValidityRange } {
  const closes: { id: string; valid_until: string }[] = [];
  const remaining: ValidityRange[] = [];

  for (const row of existing) {
    const overlaps = dateRangesOverlap(
      validFrom,
      validUntil,
      row.valid_from,
      row.valid_until,
    );
    if (
      overlaps &&
      row.valid_until == null &&
      row.id != null &&
      row.valid_from < validFrom
    ) {
      const closedUntil = dayBeforeIsoDate(validFrom);
      if (closedUntil < row.valid_from) {
        return { closes: [], conflict: row };
      }
      closes.push({ id: row.id, valid_until: closedUntil });
      remaining.push({ ...row, valid_until: closedUntil });
      continue;
    }
    remaining.push(row);
  }

  const conflict = remaining.find((row) =>
    dateRangesOverlap(validFrom, validUntil, row.valid_from, row.valid_until),
  );
  return { closes, conflict };
}
