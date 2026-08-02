import { computeCourseCompletion } from "@lindaflor/core/training/completion";
import {
  createCourseDetail,
  loadCourseDetail,
  updateCourseDetail,
} from "@lindaflor/core/training/course-detail";
import { deleteLectureFilesByCourse } from "@lindaflor/core/training/lecture-pdf";
import {
  buildCompletionByCourse,
  buildStudentVisibilityCondition,
  verifyOrgCourse,
} from "@lindaflor/core/training/utils";
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
} from "@lindaflor/db/schema/training";
import {
  abilityCan,
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import { buildYouTubeThumbnailUrl } from "@lindaflor/shared/lib/youtube-url";
import { schema } from "@lindaflor/shared/schemas/training";
import { ORPCError } from "@orpc/server";
import {
  and,
  asc,
  desc,
  eq,
  getTableColumns,
  ilike,
  inArray,
  sql,
} from "drizzle-orm";
import { Effect } from "effect";
import type { z } from "zod";

type ListCoursesInput = z.infer<typeof schema.v1.courses.list.input>;

export async function listCourses(params: {
  input: ListCoursesInput;
  organizationId: string;
  userId: string;
  ability: AppAbility;
}) {
  const { input, organizationId, userId, ability } = params;
  const { pageIndex, pageSize } = input;
  const offset = (pageIndex - 1) * pageSize;

  const isAdminList =
    abilityCan(ability, "manage", "Training") && input.filter === undefined;

  const whereCondition = isAdminList
    ? and(
        eq(training_courses.organization_id, organizationId),
        input.search
          ? ilike(training_courses.title, `%${input.search}%`)
          : undefined,
      )
    : and(
        buildStudentVisibilityCondition(organizationId, userId, input.filter),
        input.search
          ? ilike(training_courses.title, `%${input.search}%`)
          : undefined,
      );

  const courseColumns = getTableColumns(training_courses);

  const [data, countResult] = await Promise.all([
    db
      .select({
        ...courseColumns,
        enrolled: sql<boolean>`EXISTS (
              SELECT 1 FROM ${training_enrollments}
              WHERE ${training_enrollments.course_id} = ${training_courses.id}
              AND ${training_enrollments.user_id} = ${userId}
            )`,
      })
      .from(training_courses)
      .where(whereCondition)
      .orderBy(desc(training_courses.created_at))
      .limit(pageSize)
      .offset(offset),
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(training_courses)
      .where(whereCondition)
      .then((rows) => rows[0]?.count ?? 0),
  ]);

  const totalPages = Math.ceil(countResult / pageSize);

  let enrichedData: {
    id: string;
    title: string;
    description: string | null;
    is_published: boolean;
    enrolled: boolean;
    organization_id: string;
    created_by_user_id: string;
    created_at: Date;
    updated_at: Date;
    thumbnail_url: string | null;
    is_completed: boolean;
    completed_at: Date | null;
  }[];

  if (isAdminList) {
    enrichedData = data.map((course) =>
      Object.assign({}, course, {
        thumbnail_url: null,
        is_completed: false,
        completed_at: null,
      }),
    );
  } else {
    const courseIds = data.map((c) => c.id);

    const lectureRows = await db
      .select({
        course_id: training_sections.course_id,
        lecture_id: training_lectures.id,
        youtube_url: training_lectures.youtube_url,
        type: training_lectures.type,
      })
      .from(training_lectures)
      .innerJoin(
        training_modules,
        eq(training_lectures.module_id, training_modules.id),
      )
      .innerJoin(
        training_sections,
        eq(training_modules.section_id, training_sections.id),
      )
      .where(inArray(training_sections.course_id, courseIds))
      .orderBy(
        asc(training_sections.sort_order),
        asc(training_modules.sort_order),
        asc(training_lectures.sort_order),
      );

    const thumbnailByCourse = new Map<string, string | null>();
    const lectureIdsByCourse = new Map<string, string[]>();
    for (const row of lectureRows) {
      const list = lectureIdsByCourse.get(row.course_id) ?? [];
      list.push(row.lecture_id);
      lectureIdsByCourse.set(row.course_id, list);

      if (row.type === "video" && !thumbnailByCourse.has(row.course_id)) {
        thumbnailByCourse.set(
          row.course_id,
          buildYouTubeThumbnailUrl(row.youtube_url ?? ""),
        );
      }
    }
    for (const id of courseIds) {
      if (!thumbnailByCourse.has(id)) {
        thumbnailByCourse.set(id, null);
      }
    }

    const completionByCourse =
      input.filter === "enrolled"
        ? await buildCompletionByCourse(
            courseIds,
            userId,
            organizationId,
            lectureIdsByCourse,
          )
        : new Map<string, { is_completed: boolean; completed_at: Date | null }>(
            courseIds.map((id) => [
              id,
              { is_completed: false, completed_at: null },
            ]),
          );

    enrichedData = data.map((course) =>
      Object.assign({}, course, {
        thumbnail_url: thumbnailByCourse.get(course.id) ?? null,
        is_completed: completionByCourse.get(course.id)?.is_completed ?? false,
        completed_at: completionByCourse.get(course.id)?.completed_at ?? null,
      }),
    );
  }

  return schema.v1.courses.list.output.parse({
    data: enrichedData,
    meta: { totalPages },
  });
}

type GetCourseInput = z.infer<typeof schema.v1.courses.get.input>;

export async function getCourse(params: {
  input: GetCourseInput;
  organizationId: string;
  userId: string;
  ability: AppAbility;
}) {
  const { input, organizationId, userId, ability } = params;

  const [course] = await db
    .select()
    .from(training_courses)
    .where(eq(training_courses.id, input.id));

  if (!course || course.organization_id !== organizationId) {
    throw new ORPCError("NOT_FOUND", {
      message: "Curso não encontrado",
    });
  }

  const [enrollment] = await db
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

  if (!enrolled && ability.cannot("manage", "Training")) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não está inscrito neste curso",
    });
  }

  if (ability.cannot("read", subject("Training", { ...course, enrolled }))) {
    throw new ORPCError("NOT_FOUND", {
      message: "Curso não encontrado",
    });
  }

  const detail = await loadCourseDetail(db, course.id, userId);

  const lectureIds = detail.sections.flatMap((section) =>
    section.modules.flatMap((module) =>
      module.lectures.map((lecture) => lecture.id),
    ),
  );

  const quizIds = detail.sections.flatMap((section) =>
    section.modules.flatMap((module) =>
      module.lectures
        .map((lecture) => lecture.quiz?.id)
        .filter((id): id is string => id !== undefined),
    ),
  );

  const [progressRows, attemptRows] = await Promise.all([
    lectureIds.length > 0
      ? db
          .select()
          .from(training_lecture_progress)
          .where(
            and(
              eq(training_lecture_progress.user_id, userId),
              eq(training_lecture_progress.organization_id, organizationId),
              inArray(training_lecture_progress.lecture_id, lectureIds),
            ),
          )
      : Promise.resolve([]),
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

  const progressByLecture = new Map(
    progressRows.map((progress) => [progress.lecture_id, progress]),
  );

  const latestAttemptByQuiz = new Map<string, (typeof attemptRows)[number]>();
  for (const attempt of attemptRows) {
    if (!latestAttemptByQuiz.has(attempt.quiz_id)) {
      latestAttemptByQuiz.set(attempt.quiz_id, attempt);
    }
  }

  const sectionsWithProgress = detail.sections.map((section) => ({
    ...section,
    modules: section.modules.map((module) => ({
      ...module,
      lectures: module.lectures.map((lecture) => ({
        ...lecture,
        progress: progressByLecture.get(lecture.id) ?? null,
        latest_attempt: lecture.quiz
          ? (latestAttemptByQuiz.get(lecture.quiz.id) ?? null)
          : null,
      })),
    })),
  }));

  return schema.v1.courses.get.output.parse({
    ...detail,
    sections: sectionsWithProgress,
  });
}

type CreateCourseInput = z.infer<typeof schema.v1.courses.create.input>;

export async function createCourse(params: {
  input: CreateCourseInput;
  organizationId: string;
  userId: string;
}) {
  const detail = await createCourseDetail(
    params.input,
    params.organizationId,
    params.userId,
  );

  return schema.v1.courses.create.output.parse(detail);
}

type UpdateCourseInput = z.infer<typeof schema.v1.courses.update.input>;

export async function updateCourse(params: {
  input: UpdateCourseInput;
  organizationId: string;
  userId: string;
}) {
  await Effect.runPromise(
    verifyOrgCourse(params.input.id, params.organizationId),
  );

  const detail = await updateCourseDetail(
    params.input,
    params.organizationId,
    params.userId,
  );

  return schema.v1.courses.update.output.parse(detail);
}

type DeleteCourseInput = z.infer<typeof schema.v1.courses.delete.input>;

export async function deleteCourse(params: {
  input: DeleteCourseInput;
  organizationId: string;
}) {
  await Effect.runPromise(
    Effect.gen(function* () {
      yield* verifyOrgCourse(params.input.id, params.organizationId);

      yield* Effect.all([
        deleteLectureFilesByCourse(params.input.id),
        Effect.tryPromise({
          try: () =>
            db
              .delete(training_courses)
              .where(eq(training_courses.id, params.input.id))
              .then(() => undefined),
          catch: (e): Error => (e instanceof Error ? e : new Error(String(e))),
        }),
      ]);
    }),
  );

  return null;
}

async function getCourseContentIds(tx: typeof db, courseId: string) {
  const rows = await tx
    .select({
      lecture_id: training_lectures.id,
      quiz_id: training_quizzes.id,
    })
    .from(training_sections)
    .innerJoin(
      training_modules,
      eq(training_modules.section_id, training_sections.id),
    )
    .innerJoin(
      training_lectures,
      eq(training_lectures.module_id, training_modules.id),
    )
    .leftJoin(
      training_quizzes,
      eq(training_quizzes.lecture_id, training_lectures.id),
    )
    .where(eq(training_sections.course_id, courseId))
    .orderBy(
      asc(training_sections.sort_order),
      asc(training_modules.sort_order),
      asc(training_lectures.sort_order),
    );

  const lectureIds: string[] = [];
  const lectureQuizLinks: { lecture_id: string; quiz_id: string | null }[] = [];
  const quizIds: string[] = [];

  for (const row of rows) {
    lectureIds.push(row.lecture_id);
    lectureQuizLinks.push({
      lecture_id: row.lecture_id,
      quiz_id: row.quiz_id ?? null,
    });
    if (row.quiz_id) {
      quizIds.push(row.quiz_id);
    }
  }

  return { lectureIds, lectureQuizLinks, quizIds };
}

type GetCourseCertificateInput = z.infer<
  typeof schema.v1.courses.certificate.get.input
>;

export async function getCourseCertificate(params: {
  input: GetCourseCertificateInput;
  organizationId: string;
  userId: string;
  ability: AppAbility;
}) {
  const { input, organizationId, userId, ability } = params;

  const [course] = await db
    .select()
    .from(training_courses)
    .where(eq(training_courses.id, input.id));

  if (!course || course.organization_id !== organizationId) {
    throw new ORPCError("NOT_FOUND", {
      message: "Curso não encontrado",
    });
  }

  const [enrollment] = await db
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

  if (ability.cannot("read", subject("Training", { ...course, enrolled }))) {
    throw new ORPCError("NOT_FOUND", {
      message: "Curso não encontrado",
    });
  }

  if (
    ability.cannot("certificate", subject("Training", { ...course, enrolled }))
  ) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não está inscrito neste curso",
    });
  }

  const { lectureIds, lectureQuizLinks, quizIds } = await getCourseContentIds(
    db,
    course.id,
  );

  const [completedProgress, attempts] = await Promise.all([
    lectureIds.length > 0
      ? db
          .select()
          .from(training_lecture_progress)
          .where(
            and(
              eq(training_lecture_progress.user_id, userId),
              eq(training_lecture_progress.organization_id, organizationId),
              inArray(training_lecture_progress.lecture_id, lectureIds),
              eq(training_lecture_progress.status, "completed"),
            ),
          )
      : Promise.resolve([]),
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

  const latestAttemptByQuiz = new Map<string, (typeof attempts)[number]>();
  for (const attempt of attempts) {
    if (!latestAttemptByQuiz.has(attempt.quiz_id)) {
      latestAttemptByQuiz.set(attempt.quiz_id, attempt);
    }
  }

  const lectureQuizAttempts = lectureQuizLinks.map(
    ({ lecture_id, quiz_id }) => ({
      lecture_id,
      latestAttempt: quiz_id
        ? (latestAttemptByQuiz.get(quiz_id) ?? null)
        : null,
    }),
  );

  const { is_completed: isCompleted, completed_at: completedAt } =
    computeCourseCompletion({
      lectureIds,
      completedProgress,
      lectureQuizAttempts,
    });

  return schema.v1.courses.certificate.get.output.parse({
    course: { ...course, enrolled },
    completed_at: completedAt,
    is_completed: isCompleted,
  });
}
