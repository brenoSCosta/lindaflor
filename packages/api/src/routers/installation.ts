import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createInstallation,
  deleteInstallations,
  getAllInstallations,
  getInstallationById,
  updateInstallation,
} from "@lindaflor/core/installation/installations";
import { schema } from "@lindaflor/shared/schemas/installation";
import type { EnhancedRouter } from "@orpc/server";

const installationsV1Routes = {
  getAll: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("read", "Installations"))
    .route({
      method: "GET",
      path: "/installations",
      description: "Get all installations",
      summary: "v1 GetAll",
    })
    .input(schema.v1.getAll.input)
    .output(schema.v1.getAll.output)
    .handler(async ({ input, context }) =>
      getAllInstallations({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  getById: authorizedProcedure
    .route({
      method: "GET",
      path: "/installations/{id}",
      description: "Get installation by ID",
      summary: "v1 GetById",
    })
    .input(schema.v1.getById.input)
    .output(schema.v1.getById.output)
    .handler(async ({ input, context }) =>
      getInstallationById({ input, ability: context.ability }),
    ),

  create: authorizedProcedure
    .use(authorize("create", "Installations"))
    .use(requireActiveOrganization())
    .route({
      method: "POST",
      path: "/installations",
      description: "Create an installation",
      summary: "v1 Create",
    })
    .input(schema.v1.create.input)
    .output(schema.v1.create.output)
    .handler(async ({ input, context }) =>
      createInstallation({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/installations",
      description: "Update an installation",
      summary: "v1 Update",
    })
    .input(schema.v1.update.input)
    .output(schema.v1.update.output)
    .handler(async ({ input, context }) =>
      updateInstallation({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/installations",
      description: "Delete installations",
      summary: "v1 Delete",
    })
    .input(schema.v1.delete.input)
    .output(schema.v1.delete.output)
    .handler(async ({ input, context }) =>
      deleteInstallations({ input, ability: context.ability }),
    ),
};

type InstallationsV1Routes = typeof installationsV1Routes;

type InstallationsV1Router = EnhancedRouter<
  InstallationsV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type InstallationsRouter = {
  v1: InstallationsV1Router;
};

function createInstallationsV1Router(
  routes: InstallationsV1Routes,
): InstallationsV1Router {
  return o.prefix("/v1").tag("Installations").router(routes);
}

export const installationsRouter: InstallationsRouter = {
  v1: createInstallationsV1Router(installationsV1Routes),
};
