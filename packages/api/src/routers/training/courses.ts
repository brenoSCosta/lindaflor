import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import { manageTrainingProcedure } from "@lindaflor/api/routers/training/utils";
import {
  createCourse,
  deleteCourse,
  getCourse,
  getCourseCertificate,
  listCourses,
  updateCourse,
} from "@lindaflor/core/training/courses";
import { schema } from "@lindaflor/shared/schemas/training";

export const coursesRouter = {
  list: authorizedProcedure
    .use(requireActiveOrganization())
    .route({
      method: "GET",
      path: "/training/courses",
      description: "List training courses",
      summary: "v1 List Courses",
    })
    .input(schema.v1.courses.list.input)
    .output(schema.v1.courses.list.output)
    .handler(async ({ input, context }) =>
      listCourses({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
        ability: context.ability,
      }),
    ),

  get: authorizedProcedure
    .use(requireActiveOrganization())
    .route({
      method: "GET",
      path: "/training/courses/{id}",
      description: "Get a training course with progress",
      summary: "v1 Get Course",
    })
    .input(schema.v1.courses.get.input)
    .output(schema.v1.courses.get.output)
    .handler(async ({ input, context }) =>
      getCourse({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
        ability: context.ability,
      }),
    ),

  create: manageTrainingProcedure
    .route({
      method: "POST",
      path: "/training/courses",
      description: "Create a training course",
      summary: "v1 Create Course",
    })
    .input(schema.v1.courses.create.input)
    .output(schema.v1.courses.create.output)
    .handler(async ({ input, context }) =>
      createCourse({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
      }),
    ),

  update: manageTrainingProcedure
    .route({
      method: "PATCH",
      path: "/training/courses",
      description: "Update a training course",
      summary: "v1 Update Course",
    })
    .input(schema.v1.courses.update.input)
    .output(schema.v1.courses.update.output)
    .handler(async ({ input, context }) =>
      updateCourse({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
      }),
    ),

  delete: manageTrainingProcedure
    .route({
      method: "DELETE",
      path: "/training/courses",
      description: "Delete a training course",
      summary: "v1 Delete Course",
    })
    .input(schema.v1.courses.delete.input)
    .output(schema.v1.courses.delete.output)
    .handler(async ({ input, context }) =>
      deleteCourse({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  certificate: {
    get: authorizedProcedure
      .use(requireActiveOrganization())
      .route({
        method: "GET",
        path: "/training/courses/{id}/certificate",
        description: "Get certificate completion status for a course",
        summary: "v1 Get Course Certificate",
      })
      .input(schema.v1.courses.certificate.get.input)
      .output(schema.v1.courses.certificate.get.output)
      .handler(async ({ input, context }) =>
        getCourseCertificate({
          input,
          organizationId: context.activeOrganizationId,
          userId: context.session.user.id,
          ability: context.ability,
        }),
      ),
  },
};
