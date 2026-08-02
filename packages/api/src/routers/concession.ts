import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createConcession,
  deleteConcessions,
  getAllConcessions,
  getConcessionById,
  updateConcession,
} from "@lindaflor/core/concession/concessions";
import { schema } from "@lindaflor/shared/schemas/concession";
import type { EnhancedRouter } from "@orpc/server";

const concessionsV1Routes = {
  getAll: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("read", "Concessions"))
    .route({
      method: "GET",
      path: "/concessions",
      description: "Get all concessions",
      summary: "v1 GetAll",
    })
    .input(schema.v1.getAll.input)
    .output(schema.v1.getAll.output)
    .handler(async ({ input, context }) =>
      getAllConcessions({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  getById: authorizedProcedure
    .route({
      method: "GET",
      path: "/concessions/{id}",
      description: "Get concession by ID",
      summary: "v1 GetById",
    })
    .input(schema.v1.getById.input)
    .output(schema.v1.getById.output)
    .handler(async ({ input, context }) =>
      getConcessionById({ input, ability: context.ability }),
    ),

  create: authorizedProcedure
    .use(authorize("create", "Concessions"))
    .use(requireActiveOrganization())
    .route({
      method: "POST",
      path: "/concessions",
      description: "Create a concession",
      summary: "v1 Create",
    })
    .input(schema.v1.create.input)
    .output(schema.v1.create.output)
    .handler(async ({ input, context }) =>
      createConcession({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/concessions",
      description: "Update a concession",
      summary: "v1 Update",
    })
    .input(schema.v1.update.input)
    .output(schema.v1.update.output)
    .handler(async ({ input, context }) =>
      updateConcession({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/concessions",
      description: "Delete concessions",
      summary: "v1 Delete",
    })
    .input(schema.v1.delete.input)
    .output(schema.v1.delete.output)
    .handler(async ({ input, context }) =>
      deleteConcessions({ input, ability: context.ability }),
    ),
};

type ConcessionsV1Routes = typeof concessionsV1Routes;

type ConcessionsV1Router = EnhancedRouter<
  ConcessionsV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type ConcessionsRouter = {
  v1: ConcessionsV1Router;
};

function createConcessionsV1Router(
  routes: ConcessionsV1Routes,
): ConcessionsV1Router {
  return o.prefix("/v1").tag("Concessions").router(routes);
}

export const concessionsRouter: ConcessionsRouter = {
  v1: createConcessionsV1Router(concessionsV1Routes),
};
