import {
  bestEffortDeleteFile,
  deleteLectureFilesByModules,
  deleteLectureFilesBySections,
  EMPTY_PDF_FIELDS,
  isTmpPdfKey,
  moveTmpPdfToPermanent,
  type ResolvedPdfLecture,
} from "@lindaflor/core/training/lecture-pdf";
import { db } from "@lindaflor/db";
import {
  training_courses,
  training_lectures,
  training_enrollments,
  training_modules,
  training_question_options,
  training_questions,
  training_quizzes,
  training_sections,
  type TrainingCourse,
  type TrainingEnrollment,
  type TrainingLecture,
  type TrainingModule,
  type TrainingQuestion,
  type TrainingQuestionOption,
  type TrainingQuiz,
  type TrainingSection,
} from "@lindaflor/db/schema/training";
import { normalizeYouTubeUrl } from "@lindaflor/shared/lib/youtube-url";
import {
  schema,
  type TrainingCourseDetailOutput,
} from "@lindaflor/shared/schemas/training";
import { ORPCError } from "@orpc/server";
import { and, asc, eq, inArray } from "drizzle-orm";
import type { z } from "zod";

const buildCourseDetail = (
  course: TrainingCourse,
  sections: TrainingSection[],
  modules: TrainingModule[],
  lectures: TrainingLecture[],
  quizzes: TrainingQuiz[],
  questions: TrainingQuestion[],
  options: TrainingQuestionOption[],
  enrollment: TrainingEnrollment | null,
): TrainingCourseDetailOutput => {
  const optionsByQuestion = new Map<string, TrainingQuestionOption[]>();
  for (const option of options) {
    const list = optionsByQuestion.get(option.question_id) ?? [];
    list.push(option);
    optionsByQuestion.set(option.question_id, list);
  }

  const questionsWithOptions = questions.map((question) => ({
    ...question,
    options: optionsByQuestion.get(question.id) ?? [],
  }));

  const quizByLecture = new Map<string, TrainingQuiz>();
  for (const quiz of quizzes) {
    quizByLecture.set(quiz.lecture_id, quiz);
  }

  const lecturesWithChildren = lectures.map((lecture) => {
    const quiz = quizByLecture.get(lecture.id);
    const quizWithQuestions = quiz
      ? {
          ...quiz,
          description: quiz.description ?? undefined,
          questions: questionsWithOptions.filter(
            (question) => question.quiz_id === quiz.id,
          ),
        }
      : undefined;

    return {
      ...lecture,
      youtube_url: lecture.youtube_url ?? undefined,
      file_key: lecture.file_key ?? undefined,
      file_name: lecture.file_name ?? undefined,
      file_size: lecture.file_size,
      mime_type: lecture.mime_type ?? undefined,
      url: lecture.url ?? undefined,
      progress: null,
      quiz: quizWithQuestions,
      latest_attempt: null,
    };
  });

  const lecturesWithChildrenByModule = new Map<
    string,
    typeof lecturesWithChildren
  >();
  for (const lecture of lecturesWithChildren) {
    const list = lecturesWithChildrenByModule.get(lecture.module_id) ?? [];
    list.push(lecture);
    lecturesWithChildrenByModule.set(lecture.module_id, list);
  }

  const modulesWithChildren = modules.map((module) => ({
    ...module,
    description: module.description ?? undefined,
    lectures: lecturesWithChildrenByModule.get(module.id) ?? [],
  }));

  const modulesWithChildrenBySection = new Map<
    string,
    typeof modulesWithChildren
  >();
  for (const module of modulesWithChildren) {
    const list = modulesWithChildrenBySection.get(module.section_id) ?? [];
    list.push(module);
    modulesWithChildrenBySection.set(module.section_id, list);
  }

  const sectionsWithChildren = sections.map((section) => ({
    ...section,
    description: section.description ?? undefined,
    modules: modulesWithChildrenBySection.get(section.id) ?? [],
  }));

  return {
    ...course,
    description: course.description ?? undefined,
    enrolled: enrollment !== null,
    sections: sectionsWithChildren,
    enrollment,
  };
};

type CreateCourseInput = z.infer<typeof schema.v1.courses.create.input>;

type UpdateCourseInput = z.infer<typeof schema.v1.courses.update.input>;

type LectureTreeLecture =
  CreateCourseInput["sections"][number]["modules"][number]["lectures"][number];

const moveTmpPdfFilesInTree = async (
  sections: {
    modules: { lectures: LectureTreeLecture[] }[];
  }[],
): Promise<void> => {
  const pending: Array<{
    lecture: LectureTreeLecture;
    tmp_key: string;
    file_name: string;
  }> = [];

  for (const section of sections) {
    for (const module of section.modules) {
      for (const lecture of module.lectures) {
        if (lecture.type !== "pdf" || !lecture.file_key) {
          continue;
        }
        if (!isTmpPdfKey(lecture.file_key)) {
          continue;
        }
        pending.push({
          lecture,
          tmp_key: lecture.file_key,
          file_name: lecture.file_name ?? "document.pdf",
        });
      }
    }
  }

  await Promise.all(
    pending.map(async ({ lecture, tmp_key, file_name }) => {
      lecture.file_key = await moveTmpPdfToPermanent(tmp_key, file_name);
    }),
  );
};

type LectureInputFields = {
  type: "video" | "pdf" | "link";
  youtube_url?: string;
  file_key?: string;
  file_name?: string;
  file_size?: number | null;
  mime_type?: string;
  url?: string;
};

const resolveLectureFields = (
  input: LectureInputFields,
): ResolvedPdfLecture & {
  youtube_url: string | null;
  url: string | null;
} => {
  const pdf =
    input.type === "pdf" && input.file_key
      ? {
          file_key: input.file_key,
          file_name: input.file_name ?? null,
          file_size: input.file_size ?? null,
          mime_type: input.mime_type ?? null,
        }
      : EMPTY_PDF_FIELDS;

  return {
    youtube_url:
      input.type === "video" && input.youtube_url
        ? normalizeYouTubeUrl(input.youtube_url)
        : null,
    file_key: pdf.file_key,
    file_name: pdf.file_name,
    file_size: pdf.file_size,
    mime_type: pdf.mime_type,
    url: input.type === "link" ? (input.url ?? null) : null,
  };
};

const syncQuizChildren = async (
  tx: typeof db,
  quizId: string,
  questions: {
    id?: string;
    text: string;
    sort_order: number;
    options: {
      id?: string;
      text: string;
      is_correct: boolean;
      sort_order: number;
    }[];
  }[],
): Promise<void> => {
  const existingQuestions = await tx
    .select()
    .from(training_questions)
    .where(eq(training_questions.quiz_id, quizId));

  const inputQuestionIds = new Set(
    questions
      .map((question) => question.id)
      .filter((id): id is string => id !== undefined),
  );

  const questionsToDelete = existingQuestions.filter(
    (question) => !inputQuestionIds.has(question.id),
  );

  if (questionsToDelete.length > 0) {
    await tx.delete(training_questions).where(
      inArray(
        training_questions.id,
        questionsToDelete.map((question) => question.id),
      ),
    );
  }

  await Promise.all(
    questions.map(async (questionInput) => {
      let questionId: string;

      if (questionInput.id) {
        const [existingQuestion] = await tx
          .select()
          .from(training_questions)
          .where(
            and(
              eq(training_questions.id, questionInput.id),
              eq(training_questions.quiz_id, quizId),
            ),
          );

        if (!existingQuestion) {
          throw new ORPCError("NOT_FOUND", {
            message: `Question ${questionInput.id} not found`,
          });
        }

        await tx
          .update(training_questions)
          .set({
            text: questionInput.text,
            sort_order: questionInput.sort_order,
          })
          .where(eq(training_questions.id, questionInput.id));

        questionId = questionInput.id;
      } else {
        const [question] = await tx
          .insert(training_questions)
          .values({
            quiz_id: quizId,
            text: questionInput.text,
            sort_order: questionInput.sort_order,
          })
          .returning();

        if (!question) {
          throw new ORPCError("INTERNAL_SERVER_ERROR", {
            message: "Falha ao criar pergunta",
          });
        }

        questionId = question.id;
      }

      const existingOptions = await tx
        .select()
        .from(training_question_options)
        .where(eq(training_question_options.question_id, questionId));

      const inputOptionIds = new Set(
        questionInput.options
          .map((option) => option.id)
          .filter((id): id is string => id !== undefined),
      );

      const optionsToDelete = existingOptions.filter(
        (option) => !inputOptionIds.has(option.id),
      );

      if (optionsToDelete.length > 0) {
        await tx.delete(training_question_options).where(
          inArray(
            training_question_options.id,
            optionsToDelete.map((option) => option.id),
          ),
        );
      }

      await Promise.all(
        questionInput.options.map(async (optionInput) => {
          if (optionInput.id) {
            const [existingOption] = await tx
              .select()
              .from(training_question_options)
              .where(
                and(
                  eq(training_question_options.id, optionInput.id),
                  eq(training_question_options.question_id, questionId),
                ),
              );

            if (!existingOption) {
              throw new ORPCError("NOT_FOUND", {
                message: `Option ${optionInput.id} not found`,
              });
            }

            await tx
              .update(training_question_options)
              .set({
                text: optionInput.text,
                is_correct: optionInput.is_correct,
                sort_order: optionInput.sort_order,
              })
              .where(eq(training_question_options.id, optionInput.id));
          } else {
            await tx.insert(training_question_options).values({
              question_id: questionId,
              text: optionInput.text,
              is_correct: optionInput.is_correct,
              sort_order: optionInput.sort_order,
            });
          }
        }),
      );
    }),
  );
};

export const createCourseDetail = async (
  input: CreateCourseInput,
  organizationId: string,
  createdByUserId: string,
): Promise<TrainingCourseDetailOutput> => {
  await moveTmpPdfFilesInTree(input.sections);

  return db.transaction(async (tx) => {
    const [course] = await tx
      .insert(training_courses)
      .values({
        title: input.title,
        description: input.description ?? null,
        is_published: input.is_published,
        organization_id: organizationId,
        created_by_user_id: createdByUserId,
      })
      .returning();

    if (!course) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar curso",
      });
    }

    await Promise.all(
      input.sections.map(async (sectionInput) => {
        const [section] = await tx
          .insert(training_sections)
          .values({
            course_id: course.id,
            organization_id: organizationId,
            title: sectionInput.title,
            description: sectionInput.description ?? null,
            sort_order: sectionInput.sort_order,
          })
          .returning();

        if (!section) {
          throw new ORPCError("INTERNAL_SERVER_ERROR", {
            message: "Falha ao criar seção",
          });
        }

        await Promise.all(
          sectionInput.modules.map(async (moduleInput) => {
            const [module] = await tx
              .insert(training_modules)
              .values({
                section_id: section.id,
                organization_id: organizationId,
                title: moduleInput.title,
                description: moduleInput.description ?? null,
                sort_order: moduleInput.sort_order,
              })
              .returning();

            if (!module) {
              throw new ORPCError("INTERNAL_SERVER_ERROR", {
                message: "Falha ao criar módulo",
              });
            }

            await Promise.all(
              moduleInput.lectures.map(async (lectureInput) => {
                const [lecture] = await tx
                  .insert(training_lectures)
                  .values({
                    module_id: module.id,
                    organization_id: organizationId,
                    title: lectureInput.title,
                    type: lectureInput.type,
                    ...resolveLectureFields(lectureInput),
                    sort_order: lectureInput.sort_order,
                  })
                  .returning();

                if (!lecture) {
                  throw new ORPCError("INTERNAL_SERVER_ERROR", {
                    message: "Falha ao criar aula",
                  });
                }

                if (lectureInput.quiz) {
                  const [quiz] = await tx
                    .insert(training_quizzes)
                    .values({
                      lecture_id: lecture.id,
                      organization_id: organizationId,
                      title: lectureInput.quiz.title,
                      description: lectureInput.quiz.description ?? null,
                      passing_score: lectureInput.quiz.passing_score,
                    })
                    .returning();

                  if (!quiz) {
                    throw new ORPCError("INTERNAL_SERVER_ERROR", {
                      message: "Falha ao criar questionário",
                    });
                  }

                  await Promise.all(
                    lectureInput.quiz.questions.map(async (questionInput) => {
                      const [question] = await tx
                        .insert(training_questions)
                        .values({
                          quiz_id: quiz.id,
                          text: questionInput.text,
                          sort_order: questionInput.sort_order,
                        })
                        .returning();

                      if (!question) {
                        throw new ORPCError("INTERNAL_SERVER_ERROR", {
                          message: "Falha ao criar pergunta",
                        });
                      }

                      await tx.insert(training_question_options).values(
                        questionInput.options.map((option) => ({
                          question_id: question.id,
                          text: option.text,
                          is_correct: option.is_correct,
                          sort_order: option.sort_order,
                        })),
                      );
                    }),
                  );
                }
              }),
            );
          }),
        );
      }),
    );

    return loadCourseDetail(tx, course.id);
  });
};

export const loadCourseDetail = async (
  tx: typeof db,
  courseId: string,
  userId?: string,
): Promise<TrainingCourseDetailOutput> => {
  const [course] = await tx
    .select()
    .from(training_courses)
    .where(eq(training_courses.id, courseId));

  if (!course) {
    throw new ORPCError("NOT_FOUND", {
      message: "Curso não encontrado",
    });
  }

  const sections = await tx
    .select()
    .from(training_sections)
    .where(eq(training_sections.course_id, courseId))
    .orderBy(asc(training_sections.sort_order));

  const sectionIds = sections.map((section) => section.id);

  const modules =
    sectionIds.length > 0
      ? await tx
          .select()
          .from(training_modules)
          .where(inArray(training_modules.section_id, sectionIds))
          .orderBy(asc(training_modules.sort_order))
      : [];

  const moduleIds = modules.map((module) => module.id);

  const enrollmentQuery = userId
    ? tx
        .select()
        .from(training_enrollments)
        .where(
          and(
            eq(training_enrollments.course_id, courseId),
            eq(training_enrollments.user_id, userId),
          ),
        )
        .limit(1)
    : Promise.resolve([]);

  const [lectures, enrollments] = await Promise.all([
    moduleIds.length > 0
      ? tx
          .select()
          .from(training_lectures)
          .where(inArray(training_lectures.module_id, moduleIds))
          .orderBy(asc(training_lectures.sort_order))
      : Promise.resolve([]),
    enrollmentQuery,
  ]);

  const lectureIds = lectures.map((lecture) => lecture.id);

  const quizzes =
    lectureIds.length > 0
      ? await tx
          .select()
          .from(training_quizzes)
          .where(inArray(training_quizzes.lecture_id, lectureIds))
      : [];

  const quizIds = quizzes.map((quiz) => quiz.id);

  const questions =
    quizIds.length > 0
      ? await tx
          .select()
          .from(training_questions)
          .where(inArray(training_questions.quiz_id, quizIds))
          .orderBy(asc(training_questions.sort_order))
      : [];

  const questionIds = questions.map((question) => question.id);

  const options =
    questionIds.length > 0
      ? await tx
          .select()
          .from(training_question_options)
          .where(inArray(training_question_options.question_id, questionIds))
          .orderBy(asc(training_question_options.sort_order))
      : [];

  return buildCourseDetail(
    course,
    sections,
    modules,
    lectures,
    quizzes,
    questions,
    options,
    enrollments[0] ?? null,
  );
};

export const updateCourseDetail = async (
  input: UpdateCourseInput,
  organizationId: string,
  userId: string,
): Promise<TrainingCourseDetailOutput> => {
  await moveTmpPdfFilesInTree(input.sections);

  return db.transaction(async (tx) => {
    const [existingCourse] = await tx
      .select()
      .from(training_courses)
      .where(eq(training_courses.id, input.id));

    if (!existingCourse) {
      throw new ORPCError("NOT_FOUND", {
        message: "Curso não encontrado",
      });
    }

    const courseUpdate: Partial<{
      title: string;
      description: string | null;
      is_published: boolean;
      updated_at: Date;
    }> = {
      updated_at: new Date(),
    };
    if (input.title !== undefined) {
      courseUpdate.title = input.title;
    }
    if (input.description !== undefined) {
      courseUpdate.description = input.description;
    }
    if (input.is_published !== undefined) {
      courseUpdate.is_published = input.is_published;
    }

    await tx
      .update(training_courses)
      .set(courseUpdate)
      .where(eq(training_courses.id, input.id));

    const existingSections = await tx
      .select()
      .from(training_sections)
      .where(eq(training_sections.course_id, input.id));

    const inputSectionIds = new Set(
      input.sections
        .map((section) => section.id)
        .filter((id): id is string => id !== undefined),
    );

    const sectionsToDelete = existingSections.filter(
      (section) => !inputSectionIds.has(section.id),
    );

    if (sectionsToDelete.length > 0) {
      await deleteLectureFilesBySections(
        sectionsToDelete.map((section) => section.id),
        tx,
      );
      await tx.delete(training_sections).where(
        inArray(
          training_sections.id,
          sectionsToDelete.map((section) => section.id),
        ),
      );
    }

    await Promise.all(
      input.sections.map(async (sectionInput) => {
        let sectionId: string;

        if (sectionInput.id) {
          const [existingSection] = await tx
            .select()
            .from(training_sections)
            .where(
              and(
                eq(training_sections.id, sectionInput.id),
                eq(training_sections.course_id, input.id),
              ),
            );

          if (!existingSection) {
            throw new ORPCError("NOT_FOUND", {
              message: `Section ${sectionInput.id} not found`,
            });
          }

          await tx
            .update(training_sections)
            .set({
              title: sectionInput.title,
              description:
                sectionInput.description ?? existingSection.description,
              sort_order: sectionInput.sort_order,
            })
            .where(eq(training_sections.id, sectionInput.id));

          sectionId = sectionInput.id;
        } else {
          const [section] = await tx
            .insert(training_sections)
            .values({
              course_id: input.id,
              organization_id: organizationId,
              title: sectionInput.title,
              description: sectionInput.description ?? null,
              sort_order: sectionInput.sort_order,
            })
            .returning();

          if (!section) {
            throw new ORPCError("INTERNAL_SERVER_ERROR", {
              message: "Falha ao criar seção",
            });
          }

          sectionId = section.id;
        }

        const existingModules = await tx
          .select()
          .from(training_modules)
          .where(eq(training_modules.section_id, sectionId));

        const inputModuleIds = new Set(
          sectionInput.modules
            .map((module) => module.id)
            .filter((id): id is string => id !== undefined),
        );

        const modulesToDelete = existingModules.filter(
          (module) => !inputModuleIds.has(module.id),
        );

        if (modulesToDelete.length > 0) {
          await deleteLectureFilesByModules(
            modulesToDelete.map((module) => module.id),
            tx,
          );
          await tx.delete(training_modules).where(
            inArray(
              training_modules.id,
              modulesToDelete.map((module) => module.id),
            ),
          );
        }

        await Promise.all(
          sectionInput.modules.map(async (moduleInput) => {
            let moduleId: string;

            if (moduleInput.id) {
              const [existingModule] = await tx
                .select()
                .from(training_modules)
                .where(
                  and(
                    eq(training_modules.id, moduleInput.id),
                    eq(training_modules.section_id, sectionId),
                  ),
                );

              if (!existingModule) {
                throw new ORPCError("NOT_FOUND", {
                  message: `Module ${moduleInput.id} not found`,
                });
              }

              await tx
                .update(training_modules)
                .set({
                  title: moduleInput.title,
                  description:
                    moduleInput.description ?? existingModule.description,
                  sort_order: moduleInput.sort_order,
                })
                .where(eq(training_modules.id, moduleInput.id));

              moduleId = moduleInput.id;
            } else {
              const [module] = await tx
                .insert(training_modules)
                .values({
                  section_id: sectionId,
                  organization_id: organizationId,
                  title: moduleInput.title,
                  description: moduleInput.description ?? null,
                  sort_order: moduleInput.sort_order,
                })
                .returning();

              if (!module) {
                throw new ORPCError("INTERNAL_SERVER_ERROR", {
                  message: "Falha ao criar módulo",
                });
              }

              moduleId = module.id;
            }

            const existingLectures = await tx
              .select()
              .from(training_lectures)
              .where(eq(training_lectures.module_id, moduleId));

            const inputLectureIds = new Set(
              moduleInput.lectures
                .map((lecture) => lecture.id)
                .filter((id): id is string => id !== undefined),
            );

            const lecturesToDelete = existingLectures.filter(
              (lecture) => !inputLectureIds.has(lecture.id),
            );

            if (lecturesToDelete.length > 0) {
              await Promise.all(
                lecturesToDelete.map((lecture) =>
                  bestEffortDeleteFile(lecture.file_key),
                ),
              );
              await tx.delete(training_lectures).where(
                inArray(
                  training_lectures.id,
                  lecturesToDelete.map((lecture) => lecture.id),
                ),
              );
            }

            await Promise.all(
              moduleInput.lectures.map(async (lectureInput) => {
                let lectureId: string;

                if (lectureInput.id) {
                  const [existingLecture] = await tx
                    .select()
                    .from(training_lectures)
                    .where(
                      and(
                        eq(training_lectures.id, lectureInput.id),
                        eq(training_lectures.module_id, moduleId),
                      ),
                    );

                  if (!existingLecture) {
                    throw new ORPCError("NOT_FOUND", {
                      message: `Lecture ${lectureInput.id} not found`,
                    });
                  }

                  if (
                    existingLecture.file_key &&
                    existingLecture.file_key !== lectureInput.file_key
                  ) {
                    await bestEffortDeleteFile(existingLecture.file_key);
                  }

                  await tx
                    .update(training_lectures)
                    .set({
                      title: lectureInput.title,
                      type: lectureInput.type,
                      ...resolveLectureFields(lectureInput),
                      sort_order: lectureInput.sort_order,
                    })
                    .where(eq(training_lectures.id, lectureInput.id));

                  lectureId = lectureInput.id;
                } else {
                  const [lecture] = await tx
                    .insert(training_lectures)
                    .values({
                      module_id: moduleId,
                      organization_id: organizationId,
                      title: lectureInput.title,
                      type: lectureInput.type,
                      ...resolveLectureFields(lectureInput),
                      sort_order: lectureInput.sort_order,
                    })
                    .returning();

                  if (!lecture) {
                    throw new ORPCError("INTERNAL_SERVER_ERROR", {
                      message: "Falha ao criar aula",
                    });
                  }

                  lectureId = lecture.id;
                }

                // Quiz is 1:1 with a lecture (unique index on lecture_id).
                const existingQuizzes = await tx
                  .select()
                  .from(training_quizzes)
                  .where(eq(training_quizzes.lecture_id, lectureId));

                const existingQuiz = existingQuizzes[0] ?? null;

                if (lectureInput.quiz) {
                  let quizId: string;

                  if (
                    lectureInput.quiz.id &&
                    existingQuiz?.id === lectureInput.quiz.id
                  ) {
                    await tx
                      .update(training_quizzes)
                      .set({
                        title: lectureInput.quiz.title,
                        description:
                          lectureInput.quiz.description ??
                          existingQuiz.description,
                        passing_score: lectureInput.quiz.passing_score,
                      })
                      .where(eq(training_quizzes.id, existingQuiz.id));

                    quizId = existingQuiz.id;
                  } else if (existingQuiz) {
                    await tx
                      .update(training_quizzes)
                      .set({
                        title: lectureInput.quiz.title,
                        description:
                          lectureInput.quiz.description ??
                          existingQuiz.description,
                        passing_score: lectureInput.quiz.passing_score,
                      })
                      .where(eq(training_quizzes.id, existingQuiz.id));

                    quizId = existingQuiz.id;
                  } else {
                    const [quiz] = await tx
                      .insert(training_quizzes)
                      .values({
                        lecture_id: lectureId,
                        organization_id: organizationId,
                        title: lectureInput.quiz.title,
                        description: lectureInput.quiz.description ?? null,
                        passing_score: lectureInput.quiz.passing_score,
                      })
                      .returning();

                    if (!quiz) {
                      throw new ORPCError("INTERNAL_SERVER_ERROR", {
                        message: "Falha ao criar questionário",
                      });
                    }

                    quizId = quiz.id;
                  }

                  await syncQuizChildren(
                    tx,
                    quizId,
                    lectureInput.quiz.questions,
                  );
                } else if (existingQuiz) {
                  await tx
                    .delete(training_quizzes)
                    .where(eq(training_quizzes.id, existingQuiz.id));
                }
              }),
            );
          }),
        );
      }),
    );

    return loadCourseDetail(tx, input.id, userId);
  });
};
