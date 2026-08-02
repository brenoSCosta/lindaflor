import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createTank,
  deleteTanks,
  getTankById,
  getTankByTag,
  getTankSnapshot,
  listAllTanks,
  listTankSnapshots,
  updateTank,
} from "@lindaflor/core/tankage/tanks";
import { schema } from "@lindaflor/shared/schemas/tankage";

export const tankRouter = {
  list: {
    all: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tanks"))
      .route({
        method: "GET",
        path: "/tanks",
        description: "Get all tanks",
        summary: "v1 GetAll",
      })
      .input(schema.v1.tank.list.all.input)
      .output(schema.v1.tank.list.all.output)
      .handler(async ({ input, context }) =>
        listAllTanks({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),

    snapshot: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tanks"))
      .route({
        method: "GET",
        path: "/tanks/snapshots",
        description: "List tank inventory snapshots for the organization",
        summary: "v1 ListSnapshots",
      })
      .input(schema.v1.tank.list.snapshot.input)
      .output(schema.v1.tank.list.snapshot.output)
      .handler(async ({ input, context }) =>
        listTankSnapshots({
          input,
          organizationId: context.activeOrganizationId,
          timezone: context.client.timezone,
        }),
      ),
  },

  getBy: {
    id: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tanks"))
      .route({
        method: "GET",
        path: "/tanks/{id}",
        description: "Get a tank by id",
        summary: "v1 GetById",
      })
      .input(schema.v1.tank.getBy.id.input)
      .output(schema.v1.tank.getBy.id.output)
      .handler(async ({ input, context }) =>
        getTankById({ input, ability: context.ability }),
      ),

    tag: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tanks"))
      .route({
        method: "GET",
        path: "/tanks/by-tag/{tag}",
        description: "Get a tank by TAG",
        summary: "v1 GetByTag",
      })
      .input(schema.v1.tank.getBy.tag.input)
      .output(schema.v1.tank.getBy.tag.output)
      .handler(async ({ input, context }) =>
        getTankByTag({
          input,
          organizationId: context.activeOrganizationId,
          ability: context.ability,
        }),
      ),
  },

  create: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("create", "Tanks"))
    .route({
      method: "POST",
      path: "/tanks",
      description: "Create a tank",
      summary: "v1 Create",
    })
    .input(schema.v1.tank.create.input)
    .output(schema.v1.tank.create.output)
    .handler(async ({ input, context }) =>
      createTank({
        input,
        organizationId: context.activeOrganizationId,
        createdByUserId: context.session.user.id,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/tanks",
      description: "Update a tank",
      summary: "v1 Update",
    })
    .input(schema.v1.tank.update.input)
    .output(schema.v1.tank.update.output)
    .handler(async ({ input, context }) =>
      updateTank({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/tanks",
      description: "Delete tanks",
      summary: "v1 Delete",
    })
    .input(schema.v1.tank.delete.input)
    .output(schema.v1.tank.delete.output)
    .handler(async ({ input, context }) =>
      deleteTanks({ input, ability: context.ability }),
    ),

  get: {
    snapshot: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "Tanks"))
      .route({
        method: "GET",
        path: "/tanks/{id}/snapshot",
        description: "Tank inventory snapshot with volumes",
        summary: "v1 GetSnapshot",
      })
      .input(schema.v1.tank.get.snapshot.input)
      .output(schema.v1.tank.get.snapshot.output)
      .handler(async ({ input, context }) =>
        getTankSnapshot({
          input,
          organizationId: context.activeOrganizationId,
          timezone: context.client.timezone,
        }),
      ),
  },
};
