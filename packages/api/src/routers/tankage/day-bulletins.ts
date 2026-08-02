import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  approveTankDayBulletin,
  deleteTankDayBulletinOperations,
  getTankDayBulletinByDay,
  listTankDayBulletinEventsByDay,
  listTankDayBulletinsByTank,
  reopenTankDayBulletin,
} from "@lindaflor/core/tankage/day-bulletins";
import { schema } from "@lindaflor/shared/schemas/tankage";

export const bulletinRouter = {
  getBy: {
    day: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "TankDayBulletins"))
      .route({
        method: "GET",
        path: "/tank-day-bulletins/by-tank-day",
        description: "Get tank day bulletin status",
        summary: "v1 GetByTankDay",
      })
      .input(schema.v1.bulletin.getBy.day.input)
      .output(schema.v1.bulletin.getBy.day.output)
      .handler(async ({ input, context }) =>
        getTankDayBulletinByDay({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },

  event: {
    listBy: {
      day: authorizedProcedure
        .use(requireActiveOrganization())
        .use(authorize("read", "TankDayBulletins"))
        .route({
          method: "GET",
          path: "/tank-day-bulletins/events",
          description: "List audit events for a tank day bulletin",
          summary: "v1 EventListByDay",
        })
        .input(schema.v1.bulletin.event.listBy.day.input)
        .output(schema.v1.bulletin.event.listBy.day.output)
        .handler(async ({ input, context }) =>
          listTankDayBulletinEventsByDay({
            input,
            organizationId: context.activeOrganizationId,
          }),
        ),
    },
  },

  approve: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("approve", "TankDayBulletins"))
    .route({
      method: "POST",
      path: "/tank-day-bulletins/approve",
      description: "Approve tank day bulletin (supervision)",
      summary: "v1 Approve",
    })
    .input(schema.v1.bulletin.approve.input)
    .output(schema.v1.bulletin.approve.output)
    .handler(async ({ input, context }) =>
      approveTankDayBulletin({
        input,
        organizationId: context.activeOrganizationId,
        ability: context.ability,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  reopen: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("reopen", "TankDayBulletins"))
    .route({
      method: "POST",
      path: "/tank-day-bulletins/reopen",
      description: "Reopen tank day bulletin for editing (admin)",
      summary: "v1 Reopen",
    })
    .input(schema.v1.bulletin.reopen.input)
    .output(schema.v1.bulletin.reopen.output)
    .handler(async ({ input, context }) =>
      reopenTankDayBulletin({
        input,
        organizationId: context.activeOrganizationId,
        ability: context.ability,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  delete: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("delete", "TankDayBulletins"))
    .route({
      method: "POST",
      path: "/tank-day-bulletins/delete-operations",
      description:
        "Delete all tank measurements and transfers for an operational day",
      summary: "v1 DeleteOperations",
    })
    .input(schema.v1.bulletin.delete.input)
    .output(schema.v1.bulletin.delete.output)
    .handler(async ({ input, context }) =>
      deleteTankDayBulletinOperations({
        input,
        organizationId: context.activeOrganizationId,
        ability: context.ability,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  listBy: {
    tank: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "TankDayBulletins"))
      .route({
        method: "GET",
        path: "/tank-day-bulletins/by-tank",
        description: "List bulletin statuses for a tank",
        summary: "v1 ListByTank",
      })
      .input(schema.v1.bulletin.listBy.tank.input)
      .output(schema.v1.bulletin.listBy.tank.output)
      .handler(async ({ input, context }) =>
        listTankDayBulletinsByTank({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },
};
