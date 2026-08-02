import { computeCourseCompletion } from "@lindaflor/core/training/completion";
import { db } from "@lindaflor/db";
import {
  training_courses,
  training_enrollments,
  training_lecture_progress,
  training_lectures,
  training_modules,
  training_quiz_attempts,
  training_quizzes,
  training_sections,
  type TrainingCourse,
} from "@lindaflor/db/schema/training";
import {
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import { ORPCError } from "@orpc/server";
import { and, desc, eq, inArray, or, sql } from "drizzle-orm";
import { Effect } from "effect";

/**
 * Shared oRPC procedure for endpoints that require "manage" on Training.
 * Used by courses and lectures routers.
 */

/**
 * Verifies that a course exists and belongs to the given organization.
 * Throws NOT_FOUND otherwise.
 */
export const verifyOrgCourse = (courseId: string, organizationId: string) =>
  Effect.gen(function* () {
    const [course] = yield* Effect.tryPromise({
      try: () =>
        db
          .select({ id: training_courses.id })
          .from(training_courses)
          .where(
            and(
              eq(training_courses.id, courseId),
              eq(training_courses.organization_id, organizationId),
            ),
          ),
      catch: (e): Error => (e instanceof Error ? e : new Error(String(e))),
    });

    if (!course) {
      throw new ORPCError("NOT_FOUND", {
        message: "Curso não encontrado",
      });
    }
  });

/**
 * Walks the training hierarchy (lecture/quiz → module → section → course)
 * to resolve which course a lecture or quiz belongs to.
 * Uses JOINs to collapse 4-5 sequential queries into at most 2 JOIN
 * queries (one for lecture path, one for quiz path fallback).
 * Throws NOT_FOUND at any missing level.
 */
export const resolveCourseFromLectureOrQuiz = async (
  tx: typeof db,
  lectureIdOrQuizId: string,
): Promise<TrainingCourse> => {
  // Try as lecture: single JOIN through module → section → course
  const [lectureResult] = await tx
    .select({ course: training_courses })
    .from(training_lectures)
    .innerJoin(
      training_modules,
      eq(training_lectures.module_id, training_modules.id),
    )
    .innerJoin(
      training_sections,
      eq(training_modules.section_id, training_sections.id),
    )
    .innerJoin(
      training_courses,
      eq(training_sections.course_id, training_courses.id),
    )
    .where(eq(training_lectures.id, lectureIdOrQuizId))
    .limit(1);

  if (lectureResult) {
    return lectureResult.course;
  }

  // Fallback to quiz: single JOIN through lecture → module → section → course
  const [quizResult] = await tx
    .select({ course: training_courses })
    .from(training_quizzes)
    .innerJoin(
      training_lectures,
      eq(training_quizzes.lecture_id, training_lectures.id),
    )
    .innerJoin(
      training_modules,
      eq(training_lectures.module_id, training_modules.id),
    )
    .innerJoin(
      training_sections,
      eq(training_modules.section_id, training_sections.id),
    )
    .innerJoin(
      training_courses,
      eq(training_sections.course_id, training_courses.id),
    )
    .where(eq(training_quizzes.id, lectureIdOrQuizId))
    .limit(1);

  if (!quizResult) {
    throw new ORPCError("NOT_FOUND", {
      message: "Aula ou questionário não encontrado",
    });
  }

  return quizResult.course;
};

/**
 * Checks that the user is enrolled in the course associated with the given
 * lecture or quiz, and that CASL allows the "progress" action on it.
 * Throws FORBIDDEN if the check fails.
 */
export const verifyTrainingProgress = async (
  tx: typeof db,
  ability: AppAbility,
  lectureOrQuizId: string,
  organizationId: string,
  userId: string,
): Promise<void> => {
  const course = await resolveCourseFromLectureOrQuiz(tx, lectureOrQuizId);

  const [enrollment] = await tx
    .select()
    .from(training_enrollments)
    .where(
      and(
        eq(training_enrollments.course_id, course.id),
        eq(training_enrollments.user_id, userId),
        eq(training_enrollments.organization_id, organizationId),
      ),
    )
    .limit(1);

  const enrolled = enrollment !== undefined;

  if (
    ability.cannot("progress", subject("Training", { ...course, enrolled }))
  ) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não está inscrito neste curso",
    });
  }
};

/**
 * Build a Drizzle WHERE clause that restricts visible courses for
 * non-admin users based on publication status and enrollment:
 * - "enrolled"  → only courses the user is enrolled in
 * - "available" → published courses the user is NOT enrolled in
 * - undefined   → published OR enrolled courses (default student view)
 *
 * Uses EXISTS subqueries instead of prefetching enrollment IDs (O(N) rows
 * over the wire → O(1) subquery evaluation at the DB).
 */
export const buildStudentVisibilityCondition = (
  organizationId: string,
  userId: string,
  filter: "enrolled" | "available" | undefined,
) => {
  const orgFilter = eq(training_courses.organization_id, organizationId);

  const enrolledSubquery = sql<boolean>`EXISTS (
    SELECT 1 FROM ${training_enrollments}
    WHERE ${training_enrollments.course_id} = ${training_courses.id}
    AND ${training_enrollments.user_id} = ${userId}
    AND ${training_enrollments.organization_id} = ${organizationId}
  )`;

  if (filter === "enrolled") {
    return and(orgFilter, enrolledSubquery);
  }

  if (filter === "available") {
    return and(
      orgFilter,
      eq(training_courses.is_published, true),
      sql<boolean>`NOT EXISTS (
        SELECT 1 FROM ${training_enrollments}
        WHERE ${training_enrollments.course_id} = ${training_courses.id}
        AND ${training_enrollments.user_id} = ${userId}
        AND ${training_enrollments.organization_id} = ${organizationId}
      )`,
    );
  }

  return and(
    orgFilter,
    or(eq(training_courses.is_published, true), enrolledSubquery),
  );
};

/**
 * Build a map of course ID → completion status using lecture progress and
 * quiz attempt data. Only considers fully completed lectures and the
 * latest attempt per quiz. Returns a default of not-completed for any
 * course that has no data.
 */
export const buildCompletionByCourse = async (
  courseIds: string[],
  userId: string,
  organizationId: string,
  lectureIdsByCourse: Map<string, string[]>,
): Promise<
  Map<string, { is_completed: boolean; completed_at: Date | null }>
> => {
  const allLectureIds = [...lectureIdsByCourse.values()].flat();

  if (allLectureIds.length === 0) {
    return new Map(
      courseIds.map((id) => [id, { is_completed: false, completed_at: null }]),
    );
  }

  // Build inverse map once: lecture_id → course_id (O(L))
  // Used for both quiz and progress grouping — avoids O(N×C) scans
  const courseByLectureId = new Map<string, string>();
  for (const [courseId, lectureIds] of lectureIdsByCourse) {
    for (const lectureId of lectureIds) {
      courseByLectureId.set(lectureId, courseId);
    }
  }

  // Single-table quiz query (no 4-table JOIN — we map back via courseByLectureId)
  const quizRows = await db
    .select({
      id: training_quizzes.id,
      lecture_id: training_quizzes.lecture_id,
    })
    .from(training_quizzes)
    .where(inArray(training_quizzes.lecture_id, allLectureIds));

  const quizByCourse = new Map<
    string,
    { lecture_id: string; quiz_id: string }[]
  >();
  const quizIds: string[] = [];
  for (const row of quizRows) {
    const cid = courseByLectureId.get(row.lecture_id);
    if (cid) {
      const list = quizByCourse.get(cid);
      if (list) {
        list.push({ lecture_id: row.lecture_id, quiz_id: row.id });
      } else {
        quizByCourse.set(cid, [
          { lecture_id: row.lecture_id, quiz_id: row.id },
        ]);
      }
      quizIds.push(row.id);
    }
  }

  const [progressRows, attemptRows] = await Promise.all([
    db
      .select()
      .from(training_lecture_progress)
      .where(
        and(
          eq(training_lecture_progress.user_id, userId),
          eq(training_lecture_progress.organization_id, organizationId),
          inArray(training_lecture_progress.lecture_id, allLectureIds),
          eq(training_lecture_progress.status, "completed"),
        ),
      ),
    quizIds.length > 0
      ? db
          .select()
          .from(training_quiz_attempts)
          .where(
            and(
              eq(training_quiz_attempts.user_id, userId),
              eq(training_quiz_attempts.organization_id, organizationId),
              inArray(training_quiz_attempts.quiz_id, quizIds),
            ),
          )
          .orderBy(desc(training_quiz_attempts.created_at))
      : Promise.resolve([]),
  ]);

  // Group progress rows by course_id in one pass (O(C)) instead of filtering N times
  const progressByCourse = new Map<string, (typeof progressRows)[number][]>();
  for (const row of progressRows) {
    const cid = courseByLectureId.get(row.lecture_id);
    if (cid) {
      const list = progressByCourse.get(cid);
      if (list) {
        list.push(row);
      } else {
        progressByCourse.set(cid, [row]);
      }
    }
  }

  // Latest attempt per quiz (first = most recent, from ORDER BY DESC)
  const latestAttemptByQuiz = new Map<string, (typeof attemptRows)[number]>();
  for (const attempt of attemptRows) {
    if (!latestAttemptByQuiz.has(attempt.quiz_id)) {
      latestAttemptByQuiz.set(attempt.quiz_id, attempt);
    }
  }

  const completionByCourse = new Map<
    string,
    { is_completed: boolean; completed_at: Date | null }
  >();

  for (const courseId of courseIds) {
    const lectureIds = lectureIdsByCourse.get(courseId) ?? [];
    const completedProgress = progressByCourse.get(courseId) ?? [];
    const courseQuizLinks = quizByCourse.get(courseId) ?? [];
    const lectureQuizAttempts = courseQuizLinks.map(
      ({ lecture_id, quiz_id }) => ({
        lecture_id,
        latestAttempt: latestAttemptByQuiz.get(quiz_id) ?? null,
      }),
    );

    completionByCourse.set(
      courseId,
      computeCourseCompletion({
        lectureIds,
        completedProgress,
        lectureQuizAttempts,
      }),
    );
  }

  return completionByCourse;
};
