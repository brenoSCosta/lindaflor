import { formatInTimeZone } from "date-fns-tz";

const DEFAULT_DATETIME_FORMAT = "dd/MM/yyyy HH:mm";

/**
 * Formats a date in the given IANA timezone using date-fns-tz, mirroring the
 * approach used by the {@link Time} component so report output (workers, PDF)
 * stays consistent with on-screen rendering.
 */
export function formatDateTime(
  value: Date | string | number,
  timezone: string,
  formatStr: string = DEFAULT_DATETIME_FORMAT,
): string {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return formatInTimeZone(date, timezone, formatStr);
}
