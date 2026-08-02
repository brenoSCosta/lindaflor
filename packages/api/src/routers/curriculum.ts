import type { Context } from "@lindaflor/api/context";
import { o, publicProcedure } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import {
  deleteCurriculum,
  getCurriculumById,
  getCurriculumDownloadUrl,
  listCurriculums,
  submitCurriculum,
} from "@lindaflor/core/curriculum/curriculums";
import { schema } from "@lindaflor/shared/schemas/curriculum";
import type { EnhancedRouter } from "@orpc/server";

const curriculumV1Routes = {
  submit: publicProcedure
    .route({
      method: "POST",
      path: "/curriculums",
      description: "Submit curriculum",
      summary: "v1 Submit",
    })
    .input(schema.v1.submit.input)
    .output(schema.v1.submit.output)
    .handler(async ({ input }) => submitCurriculum({ input })),

  list: authorizedProcedure
    .use(authorize("read", "Curriculum"))
    .route({
      method: "GET",
      path: "/curriculums",
      description: "List curriculums",
      summary: "v1 GetAll",
    })
    .input(schema.v1.list.input)
    .output(schema.v1.list.output)
    .handler(async ({ input }) => listCurriculums({ input })),

  getById: authorizedProcedure
    .use(authorize("read", "Curriculum"))
    .route({
      method: "GET",
      path: "/curriculums/{id}",
      description: "Get curriculum by ID",
      summary: "v1 GetById",
    })
    .input(schema.v1.getById.input)
    .output(schema.v1.getById.output)
    .handler(async ({ input, context }) =>
      getCurriculumById({ input, ability: context.ability }),
    ),

  getDownloadUrl: authorizedProcedure
    .use(authorize("read", "Curriculum"))
    .route({
      method: "GET",
      path: "/curriculums/{id}/download",
      description: "Get curriculum download URL",
      summary: "v1 GetUrlById",
    })
    .input(schema.v1.getDownloadUrl.input)
    .output(schema.v1.getDownloadUrl.output)
    .handler(async ({ input, context }) =>
      getCurriculumDownloadUrl({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .use(authorize("delete", "Curriculum"))
    .route({
      method: "DELETE",
      path: "/curriculums",
      description: "Delete curriculum",
      summary: "v1 Delete",
    })
    .input(schema.v1.delete.input)
    .output(schema.v1.delete.output)
    .handler(async ({ input, context }) =>
      deleteCurriculum({ input, ability: context.ability }),
    ),
};

type CurriculumV1Routes = typeof curriculumV1Routes;

type CurriculumV1Router = EnhancedRouter<
  CurriculumV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type CurriculumRouter = {
  v1: CurriculumV1Router;
};

function createCurriculumV1Router(
  routes: CurriculumV1Routes,
): CurriculumV1Router {
  return o.prefix("/v1").tag("Curriculum").router(routes);
}

export const curriculumRouter: CurriculumRouter = {
  v1: createCurriculumV1Router(curriculumV1Routes),
};
