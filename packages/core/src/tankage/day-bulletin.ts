import { db } from "@lindaflor/db";
import { tank_day_bulletins } from "@lindaflor/db/schema/tankage";
import { and, eq } from "drizzle-orm";

export async function getTankDayBulletinStatus(args: {
  organizationId: string;
  tankId: string;
  operationalDay: string;
}): Promise<"open" | "approved"> {
  const [row] = await db
    .select({ status: tank_day_bulletins.status })
    .from(tank_day_bulletins)
    .where(
      and(
        eq(tank_day_bulletins.organization_id, args.organizationId),
        eq(tank_day_bulletins.tank_id, args.tankId),
        eq(tank_day_bulletins.operational_day, args.operationalDay),
      ),
    )
    .limit(1);

  return row?.status ?? "open";
}
