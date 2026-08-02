import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createTransfer,
  deleteTransfer,
  listTransfersByTank,
  retreatTransfer,
} from "@lindaflor/core/tankage/transfers";
import { schema } from "@lindaflor/shared/schemas/tankage";

export const transferRouter = {
  listBy: {
    tank: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "TankTransfers"))
      .route({
        method: "GET",
        path: "/tank-transfers",
        description: "List tank transfers for a tank",
        summary: "v1 ListByTank",
      })
      .input(schema.v1.transfer.listBy.tank.input)
      .output(schema.v1.transfer.listBy.tank.output)
      .handler(async ({ input, context }) =>
        listTransfersByTank({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },

  create: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("create", "TankTransfers"))
    .route({
      method: "POST",
      path: "/tank-transfers",
      description: "Create a tank transfer",
      summary: "v1 Create",
    })
    .input(schema.v1.transfer.create.input)
    .output(schema.v1.transfer.create.output)
    .handler(async ({ input, context }) =>
      createTransfer({
        input,
        organizationId: context.activeOrganizationId,
        ability: context.ability,
        timezone: context.client.timezone,
        createdByUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),

  retreat: authorizedProcedure
    .use(authorize("retreat", "TankTransfers"))
    .route({
      method: "POST",
      path: "/tank-transfers/retreat",
      description: "Retratamento of an approved-day tank transfer",
      summary: "v1 Retreat",
    })
    .input(schema.v1.transfer.retreat.input)
    .output(schema.v1.transfer.retreat.output)
    .handler(async ({ input, context }) =>
      retreatTransfer({
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
      path: "/tank-transfers",
      description: "Delete a tank transfer and linked measurement",
      summary: "v1 Delete",
    })
    .input(schema.v1.transfer.delete.input)
    .output(schema.v1.transfer.delete.output)
    .handler(async ({ input, context }) =>
      deleteTransfer({
        input,
        ability: context.ability,
        actorUserId: context.session.user.id,
        actorName: context.session.user.name,
      }),
    ),
};
