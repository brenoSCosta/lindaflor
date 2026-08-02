import { db } from "@lindaflor/db";
import { tankages, tank_transfers } from "@lindaflor/db/schema/tankage";
import { MAX_TANKAGE_MEASUREMENTS_PER_DAY } from "@lindaflor/shared/constants/tankage";
import { ORPCError } from "@orpc/server";
import { and, count, eq, isNull, ne, type SQL } from "drizzle-orm";

/**
 * Counts production tankages on an operational day.
 * Rows created by a transferência (linked via tank_transfers.tankage_id) are
 * excluded — transfers sit outside the daily production quota.
 */
export async function countTankagesOnOperationalDay(args: {
  organizationId: string;
  tankId: string;
  operationalDay: string;
  excludeId?: string;
}): Promise<number> {
  const filters: SQL[] = [
    eq(tankages.organization_id, args.organizationId),
    eq(tankages.tank_id, args.tankId),
    eq(tankages.operational_day, args.operationalDay),
    isNull(tank_transfers.id),
  ];
  if (args.excludeId != null) {
    filters.push(ne(tankages.id, args.excludeId));
  }

  const [row] = await db
    .select({ count: count() })
    .from(tankages)
    .leftJoin(tank_transfers, eq(tank_transfers.tankage_id, tankages.id))
    .where(and(...filters));

  return row?.count ?? 0;
}

export async function assertTankDayMeasurementCapacity(args: {
  organizationId: string;
  tankId: string;
  operationalDay: string;
  excludeId?: string;
}): Promise<void> {
  const existing = await countTankagesOnOperationalDay(args);
  if (existing >= MAX_TANKAGE_MEASUREMENTS_PER_DAY) {
    throw new ORPCError("CONFLICT", {
      message: `Limite de ${MAX_TANKAGE_MEASUREMENTS_PER_DAY} medições de produção por tanque e dia atingido`,
    });
  }
}
