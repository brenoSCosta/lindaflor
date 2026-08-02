import { addDays, format, parse } from "date-fns";
import { fromZonedTime } from "date-fns-tz";
import { Effect } from "effect";
import { z } from "zod";

export const DATE_RANGE_SEPARATOR = ".." as const;
export const DATE_MULTIPLE_SEPARATOR = "," as const;

function isValidIanaTimeZone(tz: string): boolean {
  return Effect.runSync(
    Effect.try({
      try: () => {
        Intl.DateTimeFormat(undefined, { timeZone: tz });
        return true;
      },
      catch: () => new Error("invalid IANA timezone"),
    }).pipe(Effect.catchAll(() => Effect.succeed(false))),
  );
}

export const ianaTimeZoneSchema = z
  .string()
  .min(1)
  .refine(isValidIanaTimeZone, { message: "Fuso horário IANA inválido" });

const isoDate = z.iso.date();

export type ParsedDateFilter = { from: string; to?: string };

type ParsedDateFilterResult = ParsedDateFilter | ParsedDateFilter[];

/**
 * Parses a raw date filter value into structured date filter objects.
 *
 * The function supports the following input formats:
 * - Range: A string containing a range separator (e.g. "2024-05-01..2024-05-10")
 * - Multiple: A string containing multiple values separated by commas (e.g. "2024-05-01,2024-05-03")
 * - Single: A string with a single ISO date (e.g. "2024-05-01")
 * - Array: An array of ISO date strings (e.g. ["2024-05-01", "2024-05-03"])
 * - Object: An object with `from` (and optionally `to`) ISO date strings
 *
 * In all cases, invalid or unparseable dates are filtered out.
 *
 * @param raw - The raw input value to parse (string, array, or object)
 * @returns A normalized date filter result, or undefined if the input is invalid
 */
export function parseDateFilterValue(
  raw: unknown,
): ParsedDateFilterResult | undefined {
  if (typeof raw === "string") {
    if (raw.includes(DATE_RANGE_SEPARATOR)) {
      const [a, b] = raw.split(DATE_RANGE_SEPARATOR).map((s) => s.trim());
      if (!a || !b) return undefined;
      const parsed = z
        .object({ from: isoDate, to: isoDate })
        .safeParse({ from: a, to: b });
      if (!parsed.success) return undefined;
      const from = a;
      const to = b;
      if (from > to) return { from: to, to: from };
      return { from, to };
    }
    if (raw.includes(DATE_MULTIPLE_SEPARATOR)) {
      const parts = raw.split(DATE_MULTIPLE_SEPARATOR).flatMap((s) => {
        const trimmed = s.trim();
        return trimmed ? [trimmed] : [];
      });
      const valid: string[] = [];
      for (const s of parts) {
        if (isoDate.safeParse(s).success) valid.push(s);
      }
      const deduped = [...new Set(valid)];
      return deduped.length > 0 ? deduped.map((from) => ({ from })) : undefined;
    }
    return isoDate.safeParse(raw).success ? { from: raw } : undefined;
  }
  if (Array.isArray(raw)) {
    const valid: string[] = [];
    for (const x of raw) {
      if (typeof x === "string" && isoDate.safeParse(x).success) valid.push(x);
    }
    const deduped = [...new Set(valid)];
    return deduped.length > 0 ? deduped.map((from) => ({ from })) : undefined;
  }
  if (typeof raw === "object" && raw !== null && "from" in raw) {
    const obj = raw as { from?: unknown; to?: unknown };
    const from = obj.from;
    const to = obj.to;
    if (typeof from !== "string") return undefined;
    const fromParsed = isoDate.safeParse(from);
    if (!fromParsed.success) return undefined;
    if (to === undefined) return { from: from };
    if (typeof to !== "string") return undefined;
    const toParsed = isoDate.safeParse(to);
    if (!toParsed.success) return undefined;
    if (from > to) return { from: to, to: from };
    return { from, to };
  }
  return undefined;
}

/**
 * Resolves a parsed date filter (yyyy-MM-dd) to UTC bounds for DB comparison.
 * When timeZone is provided, dates are interpreted as that zone's local day;
 * otherwise they are interpreted as UTC.
 */
export function parsedDateFilterToUtcRange(
  parsed: { from: string; to?: string },
  timeZone?: string,
): { fromUtc: Date; toUtcExclusive: Date } {
  if (timeZone?.trim()) {
    const tz = timeZone.trim();
    const fromUtc = fromZonedTime(parsed.from + "T00:00:00", tz);
    if (!parsed.to) {
      const toUtcExclusive = addDays(fromUtc, 1);
      return { fromUtc, toUtcExclusive };
    }
    const nextDayStr = format(
      addDays(parse(parsed.to, "yyyy-MM-dd", new Date()), 1),
      "yyyy-MM-dd",
    );
    const toUtcExclusive = fromZonedTime(nextDayStr + "T00:00:00", tz);
    return { fromUtc, toUtcExclusive };
  }
  const fromUtc = new Date(parsed.from + "T00:00:00.000Z");
  if (!parsed.to) {
    const nextDay = new Date(fromUtc);
    nextDay.setUTCDate(nextDay.getUTCDate() + 1);
    return { fromUtc, toUtcExclusive: nextDay };
  }
  const toUtc = new Date(parsed.to + "T23:59:59.999Z");
  return { fromUtc, toUtcExclusive: new Date(toUtc.getTime() + 1) };
}
