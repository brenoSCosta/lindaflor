import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import { submitQuizAttempt } from "@lindaflor/core/training/quizzes";
import { schema } from "@lindaflor/shared/schemas/training";

export const quizzesRouter = {
  attempts: {
    submit: authorizedProcedure
      .use(requireActiveOrganization())
      .route({
        method: "POST",
        path: "/training/quizzes/attempts",
        description: "Submit a quiz attempt and score it",
        summary: "v1 Submit Quiz Attempt",
      })
      .input(schema.v1.quizzes.attempts.submit.input)
      .output(schema.v1.quizzes.attempts.submit.output)
      .handler(async ({ input, context }) =>
        submitQuizAttempt({
          input,
          organizationId: context.activeOrganizationId,
          userId: context.session.user.id,
          ability: context.ability,
        }),
      ),
  },
};
