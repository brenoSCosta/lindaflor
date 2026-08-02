import { parsedDateFilterToUtcRange } from "@lindaflor/shared/lib/date-filter";
import { and, sql, type Column, type SQL } from "drizzle-orm";

type ParsedDateFilter = { from: string; to?: string };

/**
 * Builds a single SQL condition for a date column from a parsed filter.
 */
export function dateFilterToCondition(
  column: Column,
  parsed: ParsedDateFilter,
  options: { clientTimezone?: string } = {},
): SQL | undefined {
  const { fromUtc, toUtcExclusive } = parsedDateFilterToUtcRange(
    parsed,
    options.clientTimezone,
  );
  const fromUtcIso = fromUtc.toISOString();
  const toUtcExclusiveIso = toUtcExclusive.toISOString();
  return and(
    sql`${column} >= ${sql.raw(`'${fromUtcIso}'::timestamptz`)}`,
    sql`${column} < ${sql.raw(`'${toUtcExclusiveIso}'::timestamptz`)}`,
  );
}
