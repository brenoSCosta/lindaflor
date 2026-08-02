import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createEnrollment,
  deleteEnrollment,
  listEnrollments,
  selfEnroll,
} from "@lindaflor/core/training/enrollments";
import { schema } from "@lindaflor/shared/schemas/training";

const manageEnrollmentProcedure = authorizedProcedure
  .use(requireActiveOrganization())
  .use(authorize("manage", "TrainingEnrollment"))
  .errors({
    FORBIDDEN: {
      message: "Você não tem permissão para executar esta ação",
    },
  });

export const enrollmentsRouter = {
  list: manageEnrollmentProcedure
    .route({
      method: "GET",
      path: "/training/courses/{course_id}/enrollments",
      description: "List enrollments for a training course",
      summary: "v1 List Course Enrollments",
    })
    .input(schema.v1.enrollments.list.input)
    .output(schema.v1.enrollments.list.output)
    .handler(async ({ input, context }) =>
      listEnrollments({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  create: manageEnrollmentProcedure
    .route({
      method: "POST",
      path: "/training/enrollments",
      description: "Assign a user to a training course",
      summary: "v1 Create Enrollment",
    })
    .input(schema.v1.enrollments.create.input)
    .output(schema.v1.enrollments.create.output)
    .handler(async ({ input, context }) =>
      createEnrollment({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  delete: manageEnrollmentProcedure
    .route({
      method: "DELETE",
      path: "/training/enrollments",
      description: "Remove a user enrollment from a training course",
      summary: "v1 Delete Enrollment",
    })
    .input(schema.v1.enrollments.delete.input)
    .output(schema.v1.enrollments.delete.output)
    .handler(async ({ input, context }) =>
      deleteEnrollment({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  selfEnroll: authorizedProcedure
    .use(requireActiveOrganization())
    .route({
      method: "POST",
      path: "/training/enrollments/self",
      description: "Enroll the current user in a published training course",
      summary: "v1 Self Enroll",
    })
    .input(schema.v1.enrollments.selfEnroll.input)
    .output(schema.v1.enrollments.selfEnroll.output)
    .handler(async ({ input, context }) =>
      selfEnroll({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
      }),
    ),
};
