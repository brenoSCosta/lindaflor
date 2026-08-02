import { verifyTrainingProgress } from "@lindaflor/core/training/utils";
import { db } from "@lindaflor/db";
import {
  training_question_options,
  training_questions,
  training_quiz_attempts,
  training_quizzes,
} from "@lindaflor/db/schema/training";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/training";
import { ORPCError } from "@orpc/server";
import { eq, inArray } from "drizzle-orm";
import type { z } from "zod";

type SubmitQuizAttemptInput = z.infer<
  typeof schema.v1.quizzes.attempts.submit.input
>;

export async function submitQuizAttempt(params: {
  input: SubmitQuizAttemptInput;
  organizationId: string;
  userId: string;
  ability: AppAbility;
}) {
  const { input, organizationId, userId, ability } = params;

  const attempt = await db.transaction(async (tx) => {
    const [quiz] = await tx
      .select()
      .from(training_quizzes)
      .where(eq(training_quizzes.id, input.quiz_id))
      .limit(1);

    if (!quiz) {
      throw new ORPCError("NOT_FOUND", {
        message: "Questionário não encontrado",
      });
    }

    await verifyTrainingProgress(
      tx,
      ability,
      input.quiz_id,
      organizationId,
      userId,
    );

    const questions = await tx
      .select()
      .from(training_questions)
      .where(eq(training_questions.quiz_id, quiz.id));

    const questionIds = questions.map((question) => question.id);

    const options =
      questionIds.length > 0
        ? await tx
            .select()
            .from(training_question_options)
            .where(inArray(training_question_options.question_id, questionIds))
        : [];

    const questionsWithOptions = questions.map((question) => ({
      ...question,
      options: options.filter((option) => option.question_id === question.id),
    }));

    const correctCount = questionsWithOptions.filter((question) => {
      const selectedOptionId = input.answers[question.id];
      const correctOption = question.options.find(
        (option) => option.is_correct,
      );
      return correctOption?.id === selectedOptionId;
    }).length;

    const score =
      questions.length > 0
        ? Math.round((correctCount / questions.length) * 100)
        : 0;
    const passed = score >= quiz.passing_score;

    const [insertedAttempt] = await tx
      .insert(training_quiz_attempts)
      .values({
        quiz_id: quiz.id,
        user_id: userId,
        organization_id: organizationId,
        score,
        passed,
        answers: input.answers,
      })
      .returning();

    if (!insertedAttempt) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar tentativa de questionário",
      });
    }

    return insertedAttempt;
  });

  return schema.v1.quizzes.attempts.submit.output.parse(attempt);
}
