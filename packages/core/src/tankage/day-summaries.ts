import { loadTankDaySummariesForTank } from "@lindaflor/core/tankage/day-production";
import { db } from "@lindaflor/db";
import { tanks } from "@lindaflor/db/schema/tankage";
import { schema } from "@lindaflor/shared/schemas/tankage";
import { ORPCError } from "@orpc/server";
import { and, eq } from "drizzle-orm";

export async function listTankDaySummariesByTank(args: {
  input: { tank_id: string };
  organizationId: string;
}) {
  const [tank] = await db
    .select({ id: tanks.id })
    .from(tanks)
    .where(
      and(
        eq(tanks.id, args.input.tank_id),
        eq(tanks.organization_id, args.organizationId),
      ),
    );
  if (!tank) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tanque não encontrado",
    });
  }

  const data = await loadTankDaySummariesForTank({
    organizationId: args.organizationId,
    tankId: args.input.tank_id,
  });

  return schema.v1.summary.listBy.tank.output.parse({ data });
}
