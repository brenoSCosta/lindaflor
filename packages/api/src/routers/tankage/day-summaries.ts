import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import { listTankDaySummariesByTank } from "@lindaflor/core/tankage/day-summaries";
import { schema } from "@lindaflor/shared/schemas/tankage";

export const summaryRouter = {
  listBy: {
    tank: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tanks"))
      .route({
        method: "GET",
        path: "/tank-day-summaries/by-tank",
        description:
          "Daily production summaries per operational day for a tank",
        summary: "v1 ListDaySummariesByTank",
      })
      .input(schema.v1.summary.listBy.tank.input)
      .output(schema.v1.summary.listBy.tank.output)
      .handler(async ({ input, context }) =>
        listTankDaySummariesByTank({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },
};
