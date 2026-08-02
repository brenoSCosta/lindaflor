import { verifyOrgCourse } from "@lindaflor/core/training/utils";
import { db } from "@lindaflor/db";
import { members, users } from "@lindaflor/db/schema/auth";
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
import { schema } from "@lindaflor/shared/schemas/training";
import { ORPCError } from "@orpc/server";
import { and, asc, eq, inArray } from "drizzle-orm";
import { Effect } from "effect";
import type { z } from "zod";

type ListEnrollmentsInput = z.infer<typeof schema.v1.enrollments.list.input>;

export async function listEnrollments(params: {
  input: ListEnrollmentsInput;
  organizationId: string;
}) {
  const { input, organizationId } = params;
  await Effect.runPromise(verifyOrgCourse(input.course_id, organizationId));

  const rows = await db
    .select({
      enrollment: training_enrollments,
      member: members,
      user: users,
    })
    .from(training_enrollments)
    .innerJoin(
      members,
      and(
        eq(members.user_id, training_enrollments.user_id),
        eq(members.organization_id, training_enrollments.organization_id),
      ),
    )
    .innerJoin(users, eq(users.id, training_enrollments.user_id))
    .where(
      and(
        eq(training_enrollments.course_id, input.course_id),
        eq(training_enrollments.organization_id, organizationId),
      ),
    )
    .orderBy(asc(users.name), asc(users.email));

  const data = rows.map((row) => ({
    id: row.enrollment.id,
    course_id: row.enrollment.course_id,
    user_id: row.enrollment.user_id,
    organization_id: row.enrollment.organization_id,
    enrolled_at: row.enrollment.enrolled_at,
    member_id: row.member.id,
    member_role: row.member.role,
    user_name: row.user.name,
    user_email: row.user.email,
    user_image: row.user.image,
  }));

  return schema.v1.enrollments.list.output.parse({ data });
}

type CreateEnrollmentInput = z.infer<typeof schema.v1.enrollments.create.input>;

export async function createEnrollment(params: {
  input: CreateEnrollmentInput;
  organizationId: string;
}) {
  const { input, organizationId } = params;
  await Effect.runPromise(verifyOrgCourse(input.course_id, organizationId));

  const [member] = await db
    .select()
    .from(members)
    .where(
      and(
        eq(members.user_id, input.user_id),
        eq(members.organization_id, organizationId),
      ),
    )
    .limit(1);

  if (!member) {
    throw new ORPCError("BAD_REQUEST", {
      message: "O usuário não é membro desta organização",
    });
  }

  const [enrollment] = await db
    .insert(training_enrollments)
    .values({
      course_id: input.course_id,
      user_id: input.user_id,
      organization_id: organizationId,
    })
    .onConflictDoNothing({
      target: [training_enrollments.course_id, training_enrollments.user_id],
    })
    .returning();

  if (!enrollment) {
    const [existingEnrollment] = await db
      .select()
      .from(training_enrollments)
      .where(
        and(
          eq(training_enrollments.course_id, input.course_id),
          eq(training_enrollments.user_id, input.user_id),
        ),
      )
      .limit(1);

    if (existingEnrollment) {
      return schema.v1.enrollments.create.output.parse(existingEnrollment);
    }

    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar inscrição",
    });
  }

  return schema.v1.enrollments.create.output.parse(enrollment);
}

type DeleteEnrollmentInput = z.infer<typeof schema.v1.enrollments.delete.input>;

export async function deleteEnrollment(params: {
  input: DeleteEnrollmentInput;
  organizationId: string;
}) {
  const { input, organizationId } = params;
  await Effect.runPromise(verifyOrgCourse(input.course_id, organizationId));

  await db.transaction(async (tx) => {
    const [existingEnrollment] = await tx
      .select()
      .from(training_enrollments)
      .where(
        and(
          eq(training_enrollments.course_id, input.course_id),
          eq(training_enrollments.user_id, input.user_id),
          eq(training_enrollments.organization_id, organizationId),
        ),
      );

    if (!existingEnrollment) {
      throw new ORPCError("NOT_FOUND", {
        message: "Inscrição não encontrada",
      });
    }

    const lectureRows = await tx
      .select({ id: training_lectures.id })
      .from(training_lectures)
      .innerJoin(
        training_modules,
        eq(training_lectures.module_id, training_modules.id),
      )
      .innerJoin(
        training_sections,
        eq(training_modules.section_id, training_sections.id),
      )
      .where(eq(training_sections.course_id, input.course_id));

    const lectureIds = lectureRows.map((row) => row.id);

    if (lectureIds.length > 0) {
      const quizRows = await tx
        .select({ id: training_quizzes.id })
        .from(training_quizzes)
        .where(inArray(training_quizzes.lecture_id, lectureIds));

      const quizIds = quizRows.map((row) => row.id);

      await tx
        .delete(training_lecture_progress)
        .where(
          and(
            eq(training_lecture_progress.user_id, input.user_id),
            inArray(training_lecture_progress.lecture_id, lectureIds),
          ),
        );

      if (quizIds.length > 0) {
        await tx
          .delete(training_quiz_attempts)
          .where(
            and(
              eq(training_quiz_attempts.user_id, input.user_id),
              inArray(training_quiz_attempts.quiz_id, quizIds),
            ),
          );
      }
    }

    await tx
      .delete(training_enrollments)
      .where(
        and(
          eq(training_enrollments.course_id, input.course_id),
          eq(training_enrollments.user_id, input.user_id),
          eq(training_enrollments.organization_id, organizationId),
        ),
      );
  });

  return null;
}

type SelfEnrollInput = z.infer<typeof schema.v1.enrollments.selfEnroll.input>;

export async function selfEnroll(params: {
  input: SelfEnrollInput;
  organizationId: string;
  userId: string;
}) {
  const { input, organizationId, userId } = params;

  const [course] = await db
    .select()
    .from(training_courses)
    .where(
      and(
        eq(training_courses.id, input.course_id),
        eq(training_courses.organization_id, organizationId),
      ),
    );

  if (!course || !course.is_published) {
    throw new ORPCError("NOT_FOUND", {
      message: "Curso não encontrado",
    });
  }

  const [member] = await db
    .select()
    .from(members)
    .where(
      and(
        eq(members.user_id, userId),
        eq(members.organization_id, organizationId),
      ),
    )
    .limit(1);

  if (!member) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para executar esta ação",
    });
  }

  const [enrollment] = await db
    .insert(training_enrollments)
    .values({
      course_id: course.id,
      user_id: userId,
      organization_id: organizationId,
    })
    .onConflictDoNothing({
      target: [training_enrollments.course_id, training_enrollments.user_id],
    })
    .returning();

  if (!enrollment) {
    const [existingEnrollment] = await db
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

    if (existingEnrollment) {
      return schema.v1.enrollments.selfEnroll.output.parse(existingEnrollment);
    }

    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar inscrição",
    });
  }

  return schema.v1.enrollments.selfEnroll.output.parse(enrollment);
}
