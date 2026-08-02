import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createTankage,
  deleteTankages,
  getSelectedTankages,
  getTankageById,
  listAllTankages,
  listTankagesByTank,
  retreatTankage,
  updateTankage,
} from "@lindaflor/core/tankage/tankages";
import { schema } from "@lindaflor/shared/schemas/tankage";

export const tankageRouter = {
  list: {
    all: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tankages"))
      .route({
        method: "GET",
        path: "/tankages",
        description: "Get all tankages",
        summary: "v1 GetAll",
      })
      .input(schema.v1.tankage.list.all.input)
      .output(schema.v1.tankage.list.all.output)
      .handler(async ({ input, context }) =>
        listAllTankages({
          input,
          organizationId: context.activeOrganizationId,
          timezone: context.client.timezone,
        }),
      ),
  },

  getBy: {
    id: authorizedProcedure
      .route({
        method: "GET",
        path: "/tankages/{id}",
        description: "Get tankage by ID",
        summary: "v1 GetById",
      })
      .input(schema.v1.tankage.getBy.id.input)
      .output(schema.v1.tankage.getBy.id.output)
      .handler(async ({ input, context }) =>
        getTankageById({ input, ability: context.ability }),
      ),
  },

  listBy: {
    tank: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tankages"))
      .route({
        method: "GET",
        path: "/tankages/by-tank",
        description: "List tankages by tank",
        summary: "v1 ListByTank",
      })
      .input(schema.v1.tankage.listBy.tank.input)
      .output(schema.v1.tankage.listBy.tank.output)
      .handler(async ({ input, context }) =>
        listTankagesByTank({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },

  create: authorizedProcedure
    .use(authorize("create", "Tankages"))
    .use(requireActiveOrganization())
    .route({
      method: "POST",
      path: "/tankages",
      description: "Create a tankage",
      summary: "v1 Create",
    })
    .input(schema.v1.tankage.create.input)
    .output(schema.v1.tankage.create.output)
    .handler(async ({ input, context }) =>
      createTankage({
        input,
        organizationId: context.activeOrganizationId,
        ability: context.ability,
        timezone: context.client.timezone,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/tankages",
      description: "Update a Tankage",
      summary: "v1 Update",
    })
    .input(schema.v1.tankage.update.input)
    .output(schema.v1.tankage.update.output)
    .handler(async ({ input, context }) =>
      updateTankage({
        input,
        ability: context.ability,
        timezone: context.client.timezone,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  retreat: authorizedProcedure
    .use(authorize("retreat", "Tankages"))
    .route({
      method: "POST",
      path: "/tankages/retreat",
      description: "Retratamento of an approved-day tankage measurement",
      summary: "v1 Retreat",
    })
    .input(schema.v1.tankage.retreat.input)
    .output(schema.v1.tankage.retreat.output)
    .handler(async ({ input, context }) =>
      retreatTankage({
        input,
        ability: context.ability,
        timezone: context.client.timezone,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/tankages",
      description: "Delete Tankages",
      summary: "v1 Delete",
    })
    .input(schema.v1.tankage.delete.input)
    .output(schema.v1.tankage.delete.output)
    .handler(async ({ input, context }) =>
      deleteTankages({
        input,
        ability: context.ability,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  get: {
    selected: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tankages"))
      .route({
        method: "GET",
        path: "/tankages/selected",
        description: "Get selected tankages",
        summary: "v1 GetSelected",
      })
      .input(schema.v1.tankage.get.selected.input)
      .output(schema.v1.tankage.get.selected.output)
      .handler(async ({ input, context }) =>
        getSelectedTankages({
          input,
          organizationId: context.activeOrganizationId,
          timezone: context.client.timezone,
        }),
      ),
  },
};
