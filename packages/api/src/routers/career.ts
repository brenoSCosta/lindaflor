import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { publicProcedure } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import {
  createCareer,
  deleteCareer,
  listCareers,
  updateCareer,
} from "@lindaflor/core/career/careers";
import { schema } from "@lindaflor/shared/schemas/career";
import type { EnhancedRouter } from "@orpc/server";

const careerV1Routes = {
  list: publicProcedure
    .route({
      method: "GET",
      path: "/careers",
      description: "List careers",
      summary: "v1 GetAll",
    })
    .input(schema.v1.list.input)
    .output(schema.v1.list.output)
    .handler(async ({ input }) => listCareers({ input })),

  create: authorizedProcedure
    .use(authorize("create", "Curriculum"))
    .route({
      method: "POST",
      path: "/careers",
      description: "Create career",
      summary: "v1 Create",
    })
    .input(schema.v1.create.input)
    .output(schema.v1.create.output)
    .handler(async ({ input }) => createCareer({ input })),

  update: authorizedProcedure
    .use(authorize("update", "Curriculum"))
    .route({
      method: "PATCH",
      path: "/careers",
      description: "Update career",
      summary: "v1 Update",
    })
    .input(schema.v1.update.input)
    .output(schema.v1.update.output)
    .handler(async ({ input, context }) =>
      updateCareer({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .use(authorize("delete", "Curriculum"))
    .route({
      method: "DELETE",
      path: "/careers",
      description: "Delete career",
      summary: "v1 Delete",
    })
    .input(schema.v1.delete.input)
    .output(schema.v1.delete.output)
    .handler(async ({ input, context }) =>
      deleteCareer({ input, ability: context.ability }),
    ),
};

type CareerV1Routes = typeof careerV1Routes;

type CareerV1Router = EnhancedRouter<
  CareerV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type CareerRouter = {
  v1: CareerV1Router;
};

function createCareerV1Router(routes: CareerV1Routes): CareerV1Router {
  return o.prefix("/v1").tag("Career").router(routes);
}

export const careerRouter: CareerRouter = {
  v1: createCareerV1Router(careerV1Routes),
};
