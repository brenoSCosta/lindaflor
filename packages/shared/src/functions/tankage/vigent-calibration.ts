export type CalibrationValidityRow = {
  id: string;
  tank_id: string;
  valid_from: string;
  valid_until: string | null;
};

export type CalibrationVigentStatus = "current" | "expired" | "future";

export function calibrationVigentStatus(
  validFrom: string,
  validUntil: string | null,
  onDate: string,
): CalibrationVigentStatus {
  if (validFrom > onDate) {
    return "future";
  }
  if (validUntil != null && validUntil < onDate) {
    return "expired";
  }
  return "current";
}

/**
 * Picks the vigent certificate per tank the same way as the arqueação UI:
 * among certificates valid on `onDate`, the one with the earliest `valid_from`
 * (list sorted ascending by valid_from).
 *
 * When `hasPoints` is set, prefers the earliest vigent certificate that has points;
 * falls back to the earliest vigent certificate without points.
 */
export function pickVigentCalibrationByTank<T extends CalibrationValidityRow>(
  rows: readonly T[],
  onDate: string,
  options?: {
    hasPoints?: (calibrationId: string) => boolean;
  },
): Map<string, T> {
  // Avoid Array.prototype.toSorted — not available on Hermes used by Expo.
  // oxlint-disable-next-line unicorn/no-array-sort
  const sorted = rows.slice().sort((a, b) => {
    const byTank = a.tank_id.localeCompare(b.tank_id);
    if (byTank !== 0) {
      return byTank;
    }
    return a.valid_from.localeCompare(b.valid_from);
  });

  const byTank = new Map<string, T>();
  const fallbackByTank = new Map<string, T>();

  for (const row of sorted) {
    if (
      calibrationVigentStatus(row.valid_from, row.valid_until, onDate) !==
      "current"
    ) {
      continue;
    }
    if (!fallbackByTank.has(row.tank_id)) {
      fallbackByTank.set(row.tank_id, row);
    }
    const pointsOk = options?.hasPoints?.(row.id) ?? true;
    if (pointsOk && !byTank.has(row.tank_id)) {
      byTank.set(row.tank_id, row);
    }
  }

  for (const [tankId, row] of fallbackByTank) {
    if (!byTank.has(tankId)) {
      byTank.set(tankId, row);
    }
  }
  return byTank;
}
