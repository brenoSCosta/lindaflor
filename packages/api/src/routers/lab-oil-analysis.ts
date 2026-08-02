import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createLabOilAnalysis,
  deleteLabOilAnalysis,
  getLabOilAnalysisById,
  listLabOilAnalysesByTank,
  updateLabOilAnalysis,
} from "@lindaflor/core/lab-oil-analysis/lab-oil-analyses";
import { schema } from "@lindaflor/shared/schemas/lab-oil-analysis";
import type { EnhancedRouter } from "@orpc/server";

const labOilAnalysesV1Routes = {
  listByTank: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("read", "LabOilAnalyses"))
    .route({
      method: "GET",
      path: "/lab-oil-analyses",
      description: "List lab oil analyses for a tank",
      summary: "v1 ListByTank",
    })
    .input(schema.v1.listByTank.input)
    .output(schema.v1.listByTank.output)
    .handler(async ({ input, context }) =>
      listLabOilAnalysesByTank({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  getById: authorizedProcedure
    .route({
      method: "GET",
      path: "/lab-oil-analyses/{id}",
      description: "Get lab oil analysis by ID",
      summary: "v1 GetById",
    })
    .input(schema.v1.getById.input)
    .output(schema.v1.getById.output)
    .handler(async ({ input, context }) =>
      getLabOilAnalysisById({ input, ability: context.ability }),
    ),

  create: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("create", "LabOilAnalyses"))
    .route({
      method: "POST",
      path: "/lab-oil-analyses",
      description: "Create a lab oil analysis",
      summary: "v1 Create",
    })
    .input(schema.v1.create.input)
    .output(schema.v1.create.output)
    .handler(async ({ input, context }) =>
      createLabOilAnalysis({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/lab-oil-analyses",
      description: "Update a lab oil analysis",
      summary: "v1 Update",
    })
    .input(schema.v1.update.input)
    .output(schema.v1.update.output)
    .handler(async ({ input, context }) =>
      updateLabOilAnalysis({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/lab-oil-analyses",
      description: "Delete a lab oil analysis",
      summary: "v1 Delete",
    })
    .input(schema.v1.delete.input)
    .output(schema.v1.delete.output)
    .handler(async ({ input, context }) =>
      deleteLabOilAnalysis({ input, ability: context.ability }),
    ),
};

type LabOilAnalysesV1Routes = typeof labOilAnalysesV1Routes;

type LabOilAnalysesV1Router = EnhancedRouter<
  LabOilAnalysesV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type LabOilAnalysesRouter = {
  v1: LabOilAnalysesV1Router;
};

function createLabOilAnalysesV1Router(
  routes: LabOilAnalysesV1Routes,
): LabOilAnalysesV1Router {
  return o.prefix("/v1").tag("LabOilAnalyses").router(routes);
}

export const labOilAnalysesRouter: LabOilAnalysesRouter = {
  v1: createLabOilAnalysesV1Router(labOilAnalysesV1Routes),
};
