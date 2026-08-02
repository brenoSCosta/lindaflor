import { db } from "@lindaflor/db";
import {
  training_courses,
  training_enrollments,
  training_lecture_progress,
  training_lectures,
  training_modules,
  training_question_options,
  training_questions,
  training_quiz_attempts,
  training_quizzes,
  training_sections,
} from "@lindaflor/db/schema/training";
import { ensureBucket, uploadFile } from "@lindaflor/s3";
import { PDF_MIME_TYPE } from "@lindaflor/shared/constants";
import { inArray } from "drizzle-orm";
import { Effect } from "effect";

import {
  BATCH_SIZE,
  SEED_COUNT,
  SEED_E2E_ADMIN_USER_ID,
  SEED_E2E_TRAINING_GESTAO_COURSE_ID,
  SEED_E2E_TRAINING_INTRO_COURSE_ID,
  SEED_E2E_TRAINING_INTRO_ENROLLMENT_ID,
  SEED_E2E_TRAINING_INTRO_LECTURE_1_ID,
  SEED_E2E_TRAINING_INTRO_LECTURE_2_ID,
  SEED_E2E_TRAINING_INTRO_LECTURE_3_ID,
  SEED_E2E_TRAINING_INTRO_MODULE_ID,
  SEED_E2E_TRAINING_INTRO_OPTION_CORRECT_ID,
  SEED_E2E_TRAINING_INTRO_OPTION_WRONG_ID,
  SEED_E2E_TRAINING_INTRO_QUESTION_ID,
  SEED_E2E_TRAINING_INTRO_QUIZ_ID,
  SEED_E2E_TRAINING_INTRO_SECTION_ID,
  SEED_ORG_ADMIN_ID,
  SEED_ORG_IDS,
  SEED_NOW,
} from "@/seed/constants";
import {
  getOrgOwner,
  getOrgUsers,
  pickRandom,
  randomLorem,
  randomSeedDate,
  randomSeedDateAfter,
  seedIdFor,
} from "@/seed/utils";

const YOUTUBE_URLS = [
  "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
  "https://www.youtube.com/watch?v=ysz5S6PUM-U",
  "https://www.youtube.com/watch?v=aqz-KE-bpKQ",
  "https://www.youtube.com/watch?v=ScMzIvxBSi4",
  "https://www.youtube.com/watch?v=jNQXAC9IVRw",
  "https://www.youtube.com/watch?v=M7lc1UVf-VE",
  "https://www.youtube.com/watch?v=kJQP7kiw5Fk",
  "https://www.youtube.com/watch?v=LXb3EKWsInQ",
  "https://www.youtube.com/watch?v=EngW7tLk6R8",
];

function createFakeTrainingPdfBuffer(lectureId: string): Buffer {
  const content = `%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length ${60 + lectureId.length} >>
stream
BT
/F1 24 Tf
100 700 Td
(${lectureId}) Tj
0 -36 Td
/F1 12 Tf
(Material de treinamento) Tj
ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
0000000266 00000 n
0000000470 00000 n
trailer
<< /Size 6 /Root 1 0 R >>
startxref
548
%%EOF`;
  return Buffer.from(content, "utf-8");
}

type SeededQuestion = {
  id: string;
  options: Array<{ id: string; isCorrect: boolean }>;
};

type SeededQuiz = {
  id: string;
  passingScore: number;
  questions: SeededQuestion[];
};

type SeededLecture = {
  id: string;
  quiz: SeededQuiz | null;
};

type SeededCourse = {
  id: string;
  createdAt: Date;
  isPublished: boolean;
  lectureIds: string[];
  lectureQuizzes: { lectureId: string; quiz: SeededQuiz }[];
};

type SeedCounters = {
  courses: number;
  sections: number;
  modules: number;
  lectures: number;
  quizzes: number;
  questions: number;
  options: number;
  enrollments: number;
  progress: number;
  quizAttempts: number;
};

type BatchBuffers = {
  courses: (typeof training_courses.$inferInsert)[];
  sections: (typeof training_sections.$inferInsert)[];
  modules: (typeof training_modules.$inferInsert)[];
  lectures: (typeof training_lectures.$inferInsert)[];
  quizzes: (typeof training_quizzes.$inferInsert)[];
  questions: (typeof training_questions.$inferInsert)[];
  options: (typeof training_question_options.$inferInsert)[];
  enrollments: (typeof training_enrollments.$inferInsert)[];
  progress: (typeof training_lecture_progress.$inferInsert)[];
  attempts: (typeof training_quiz_attempts.$inferInsert)[];
};

function createBuffers(): BatchBuffers {
  return {
    courses: [],
    sections: [],
    modules: [],
    lectures: [],
    quizzes: [],
    questions: [],
    options: [],
    enrollments: [],
    progress: [],
    attempts: [],
  };
}

async function flushAllBuffers(buffers: BatchBuffers): Promise<void> {
  if (buffers.courses.length > 0) {
    await db.insert(training_courses).values(buffers.courses);
    buffers.courses.length = 0;
  }
  if (buffers.sections.length > 0) {
    await db.insert(training_sections).values(buffers.sections);
    buffers.sections.length = 0;
  }
  if (buffers.modules.length > 0) {
    await db.insert(training_modules).values(buffers.modules);
    buffers.modules.length = 0;
  }
  if (buffers.lectures.length > 0) {
    await db.insert(training_lectures).values(buffers.lectures);
    buffers.lectures.length = 0;
  }
  if (buffers.quizzes.length > 0) {
    await db.insert(training_quizzes).values(buffers.quizzes);
    buffers.quizzes.length = 0;
  }
  if (buffers.questions.length > 0) {
    await db.insert(training_questions).values(buffers.questions);
    buffers.questions.length = 0;
  }
  if (buffers.options.length > 0) {
    await db.insert(training_question_options).values(buffers.options);
    buffers.options.length = 0;
  }
  if (buffers.enrollments.length > 0) {
    await db.insert(training_enrollments).values(buffers.enrollments);
    buffers.enrollments.length = 0;
  }
  if (buffers.progress.length > 0) {
    await db.insert(training_lecture_progress).values(buffers.progress);
    buffers.progress.length = 0;
  }
  if (buffers.attempts.length > 0) {
    await db.insert(training_quiz_attempts).values(buffers.attempts);
    buffers.attempts.length = 0;
  }
}

async function flushIfFull(buffers: BatchBuffers): Promise<void> {
  // When any buffer hits BATCH_SIZE, flush ALL buffers in FK-safe order.
  // Independent per-table flushing would violate FK constraints — e.g.
  // questions can't be flushed before their parent quizzes are inserted.
  const anyFull =
    buffers.courses.length >= BATCH_SIZE ||
    buffers.sections.length >= BATCH_SIZE ||
    buffers.modules.length >= BATCH_SIZE ||
    buffers.lectures.length >= BATCH_SIZE ||
    buffers.quizzes.length >= BATCH_SIZE ||
    buffers.questions.length >= BATCH_SIZE ||
    buffers.options.length >= BATCH_SIZE ||
    buffers.enrollments.length >= BATCH_SIZE ||
    buffers.progress.length >= BATCH_SIZE ||
    buffers.attempts.length >= BATCH_SIZE;

  if (anyFull) {
    await flushAllBuffers(buffers);
  }
}

function generateQuiz(
  lectureId: string,
  organizationId: string,
  lectureCreatedAt: Date,
  buffers: BatchBuffers,
  counters: SeedCounters,
): SeededQuiz {
  const quizCreatedAt = randomSeedDateAfter(lectureCreatedAt);
  const quizId = seedIdFor(quizCreatedAt);
  const passingScore = 60 + Math.floor(Math.random() * 21);
  buffers.quizzes.push({
    id: quizId,
    lecture_id: lectureId,
    organization_id: organizationId,
    title: `${randomLorem(2)} Assessment`,
    description: randomLorem(4),
    passing_score: passingScore,
    created_at: quizCreatedAt,
  });

  const questions: SeededQuestion[] = [];
  const questionCount = 2 + Math.floor(Math.random() * 3);

  for (let q = 0; q < questionCount; q++) {
    const questionCreatedAt = randomSeedDateAfter(quizCreatedAt);
    const questionId = seedIdFor(questionCreatedAt);
    buffers.questions.push({
      id: questionId,
      quiz_id: quizId,
      text: `${randomLorem(3)}?`,
      sort_order: q,
      created_at: questionCreatedAt,
    });

    const options: SeededQuestion["options"] = [];
    const optionCount = 2 + Math.floor(Math.random() * 3);
    const correctIndex = Math.floor(Math.random() * optionCount);

    for (let o = 0; o < optionCount; o++) {
      const optionCreatedAt = randomSeedDateAfter(questionCreatedAt);
      const optionId = seedIdFor(optionCreatedAt);
      const isCorrect = o === correctIndex;
      buffers.options.push({
        id: optionId,
        question_id: questionId,
        text: randomLorem(1),
        is_correct: isCorrect,
        sort_order: o,
        created_at: optionCreatedAt,
      });
      options.push({ id: optionId, isCorrect });
    }

    questions.push({ id: questionId, options });
  }

  counters.quizzes++;
  counters.questions += questions.length;
  counters.options += questions.reduce((sum, q) => sum + q.options.length, 0);

  return { id: quizId, passingScore, questions };
}

async function generateLecture(
  moduleId: string,
  organizationId: string,
  moduleCreatedAt: Date,
  lectureSortOrder: number,
  buffers: BatchBuffers,
  counters: SeedCounters,
): Promise<SeededLecture> {
  const lectureCreatedAt = randomSeedDateAfter(moduleCreatedAt);
  const lectureId = seedIdFor(lectureCreatedAt);
  // Bias towards video lectures; pdf and link are less common.
  const typeRoll = Math.random();
  const type = typeRoll < 0.7 ? "video" : typeRoll < 0.85 ? "pdf" : "link";

  let file_key: string | null = null;
  let file_name: string | null = null;
  let file_size: number | null = null;
  let mime_type: string | null = null;

  if (type === "pdf") {
    const fileName = `material_${lectureId}.pdf`;
    const pdfKey = `training/lectures/${lectureId}/${fileName}`;
    const buffer = createFakeTrainingPdfBuffer(lectureId);

    await uploadFile(pdfKey, buffer, PDF_MIME_TYPE);

    file_key = pdfKey;
    file_name = fileName;
    file_size = buffer.length;
    mime_type = PDF_MIME_TYPE;
  }

  buffers.lectures.push({
    id: lectureId,
    module_id: moduleId,
    organization_id: organizationId,
    title: randomLorem(2),
    type,
    youtube_url: type === "video" ? pickRandom(YOUTUBE_URLS) : null,
    file_key,
    file_name,
    file_size,
    mime_type,
    url:
      type === "link"
        ? `https://example.com/docs/${seedIdFor(lectureCreatedAt)}`
        : null,
    sort_order: lectureSortOrder,
    created_at: lectureCreatedAt,
    updated_at: randomSeedDateAfter(lectureCreatedAt),
  });

  counters.lectures++;

  const quiz =
    Math.random() < 0.3
      ? generateQuiz(
          lectureId,
          organizationId,
          lectureCreatedAt,
          buffers,
          counters,
        )
      : null;

  return { id: lectureId, quiz };
}

async function generateCourse(
  organizationId: string,
  ownerId: string,
  buffers: BatchBuffers,
  counters: SeedCounters,
): Promise<SeededCourse> {
  const courseCreatedAt = randomSeedDate();
  const courseId = seedIdFor(courseCreatedAt);
  const isPublished = Math.random() < 0.5;

  buffers.courses.push({
    id: courseId,
    title: randomLorem(2),
    description: randomLorem(5),
    is_published: isPublished,
    organization_id: organizationId,
    created_by_user_id: ownerId,
    created_at: courseCreatedAt,
    updated_at: randomSeedDateAfter(courseCreatedAt),
  });

  const lectureIds: string[] = [];
  const lectureQuizzes: { lectureId: string; quiz: SeededQuiz }[] = [];

  const sectionCount = 1 + Math.floor(Math.random() * 2);

  for (let s = 0; s < sectionCount; s++) {
    const sectionCreatedAt = randomSeedDateAfter(courseCreatedAt);
    const sectionId = seedIdFor(sectionCreatedAt);
    buffers.sections.push({
      id: sectionId,
      course_id: courseId,
      organization_id: organizationId,
      title: randomLorem(2),
      description: randomLorem(4),
      sort_order: s,
      created_at: sectionCreatedAt,
      updated_at: randomSeedDateAfter(sectionCreatedAt),
    });
    counters.sections++;

    const moduleCount = 1 + Math.floor(Math.random() * 2);

    for (let m = 0; m < moduleCount; m++) {
      const moduleCreatedAt = randomSeedDateAfter(sectionCreatedAt);
      const moduleId = seedIdFor(moduleCreatedAt);
      buffers.modules.push({
        id: moduleId,
        section_id: sectionId,
        organization_id: organizationId,
        title: randomLorem(2),
        description: randomLorem(4),
        sort_order: m,
        created_at: moduleCreatedAt,
        updated_at: randomSeedDateAfter(moduleCreatedAt),
      });
      counters.modules++;

      const lectureCount = 1 + Math.floor(Math.random() * 3);

      for (let l = 0; l < lectureCount; l++) {
        const seededLecture = await generateLecture(
          moduleId,
          organizationId,
          moduleCreatedAt,
          l,
          buffers,
          counters,
        );
        lectureIds.push(seededLecture.id);
        if (seededLecture.quiz) {
          lectureQuizzes.push({
            lectureId: seededLecture.id,
            quiz: seededLecture.quiz,
          });
        }
      }
    }
  }

  counters.courses++;

  return {
    id: courseId,
    createdAt: courseCreatedAt,
    isPublished,
    lectureIds,
    lectureQuizzes,
  };
}

function generateEnrollments(
  seededCourse: SeededCourse,
  organizationId: string,
  orgUserIds: string[],
  buffers: BatchBuffers,
  counters: SeedCounters,
): void {
  if (!seededCourse.isPublished || orgUserIds.length === 0) return;

  const enrollmentCount =
    1 + Math.floor(Math.random() * Math.min(4, orgUserIds.length));
  const shuffledUsers = [...orgUserIds].toSorted(() => Math.random() - 0.5);

  for (let e = 0; e < enrollmentCount; e++) {
    const userId = shuffledUsers[e];
    if (!userId) continue;
    const enrolledAt = randomSeedDateAfter(seededCourse.createdAt);

    buffers.enrollments.push({
      id: seedIdFor(enrolledAt),
      course_id: seededCourse.id,
      user_id: userId,
      organization_id: organizationId,
      enrolled_at: enrolledAt,
    });
    counters.enrollments++;

    if (seededCourse.lectureIds.length > 0) {
      const progressCount = Math.floor(
        Math.random() * (seededCourse.lectureIds.length + 1),
      );
      const shuffledLectures = [...seededCourse.lectureIds].toSorted(
        () => Math.random() - 0.5,
      );

      for (let p = 0; p < progressCount; p++) {
        const lectureId = shuffledLectures[p];
        if (!lectureId) continue;
        const isCompleted = Math.random() < 0.6;
        const progressUpdatedAt = randomSeedDateAfter(enrolledAt);

        buffers.progress.push({
          id: seedIdFor(progressUpdatedAt),
          lecture_id: lectureId,
          user_id: userId,
          organization_id: organizationId,
          status: isCompleted ? "completed" : "in_progress",
          completed_at: isCompleted ? progressUpdatedAt : null,
          updated_at: progressUpdatedAt,
        });
        counters.progress++;
      }
    }

    for (const { quiz } of seededCourse.lectureQuizzes) {
      if (Math.random() < 0.5) continue;

      const attemptCreatedAt = randomSeedDateAfter(enrolledAt);
      const willPass = Math.random() < 0.5;

      const answers: Record<string, string> = {};
      for (const question of quiz.questions) {
        const correctOption = question.options.find((o) => o.isCorrect);
        const incorrectOption = question.options.find((o) => !o.isCorrect);
        if (!correctOption || !incorrectOption) continue;
        answers[question.id] = willPass ? correctOption.id : incorrectOption.id;
      }

      const correctCount = quiz.questions.filter((q) => {
        const selected = answers[q.id];
        const correct = q.options.find((o) => o.isCorrect);
        return correct?.id === selected;
      }).length;
      const score =
        quiz.questions.length > 0
          ? Math.round((correctCount / quiz.questions.length) * 100)
          : 0;

      buffers.attempts.push({
        id: seedIdFor(attemptCreatedAt),
        quiz_id: quiz.id,
        user_id: userId,
        organization_id: organizationId,
        score,
        passed: score >= quiz.passingScore,
        answers,
        created_at: attemptCreatedAt,
      });
      counters.quizAttempts++;
    }
  }
}

const E2E_INTRO_PROGRESS_IDS = [
  "0191a000-0000-7000-0000-000000000581",
  "0191a000-0000-7000-0000-000000000582",
  "0191a000-0000-7000-0000-000000000583",
] as const;

/**
 * Deterministic courses referenced by Playwright training specs.
 */
async function seedE2eTrainingFixtures(counters: SeedCounters): Promise<void> {
  const organizationId = SEED_ORG_ADMIN_ID;
  const ownerId = getOrgOwner(organizationId);
  const adminUserId = SEED_E2E_ADMIN_USER_ID;
  const fixtureCreatedAt = SEED_NOW;
  const fixtureUpdatedAt = SEED_NOW;

  await db.insert(training_courses).values([
    {
      id: SEED_E2E_TRAINING_INTRO_COURSE_ID,
      title: "Introdução à Plataforma OG Service",
      description: "Curso de onboarding para novos usuários da plataforma.",
      is_published: true,
      organization_id: organizationId,
      created_by_user_id: ownerId,
      created_at: fixtureCreatedAt,
      updated_at: fixtureUpdatedAt,
    },
    {
      id: SEED_E2E_TRAINING_GESTAO_COURSE_ID,
      title: "Gestão de Permissões",
      description: "Permissões e papéis na organização.",
      is_published: true,
      organization_id: organizationId,
      created_by_user_id: ownerId,
      created_at: fixtureCreatedAt,
      updated_at: fixtureUpdatedAt,
    },
  ]);

  await db.insert(training_sections).values({
    id: SEED_E2E_TRAINING_INTRO_SECTION_ID,
    course_id: SEED_E2E_TRAINING_INTRO_COURSE_ID,
    organization_id: organizationId,
    title: "Primeiros passos na plataforma",
    description: null,
    sort_order: 0,
    created_at: fixtureCreatedAt,
    updated_at: fixtureUpdatedAt,
  });

  await db.insert(training_modules).values({
    id: SEED_E2E_TRAINING_INTRO_MODULE_ID,
    section_id: SEED_E2E_TRAINING_INTRO_SECTION_ID,
    organization_id: organizationId,
    title: "Fundamentos",
    description: null,
    sort_order: 0,
    created_at: fixtureCreatedAt,
    updated_at: fixtureUpdatedAt,
  });

  const lectureBase = {
    module_id: SEED_E2E_TRAINING_INTRO_MODULE_ID,
    organization_id: organizationId,
    type: "video" as const,
    youtube_url: YOUTUBE_URLS[0],
    file_key: null,
    file_name: null,
    file_size: null,
    mime_type: null,
    url: null,
    created_at: fixtureCreatedAt,
    updated_at: fixtureUpdatedAt,
  };

  await db.insert(training_lectures).values([
    {
      ...lectureBase,
      id: SEED_E2E_TRAINING_INTRO_LECTURE_1_ID,
      title: "Tour pela plataforma",
      sort_order: 0,
    },
    {
      ...lectureBase,
      id: SEED_E2E_TRAINING_INTRO_LECTURE_2_ID,
      title: "Navegação e menus",
      sort_order: 1,
    },
    {
      ...lectureBase,
      id: SEED_E2E_TRAINING_INTRO_LECTURE_3_ID,
      title: "Guia do Administrador",
      sort_order: 2,
    },
  ]);

  await db.insert(training_quizzes).values({
    id: SEED_E2E_TRAINING_INTRO_QUIZ_ID,
    lecture_id: SEED_E2E_TRAINING_INTRO_LECTURE_3_ID,
    organization_id: organizationId,
    title: "Quiz do administrador",
    description: null,
    passing_score: 60,
    created_at: fixtureCreatedAt,
  });

  await db.insert(training_questions).values({
    id: SEED_E2E_TRAINING_INTRO_QUESTION_ID,
    quiz_id: SEED_E2E_TRAINING_INTRO_QUIZ_ID,
    text: "Qual papel gerencia permissões?",
    sort_order: 0,
    created_at: fixtureCreatedAt,
  });

  await db.insert(training_question_options).values([
    {
      id: SEED_E2E_TRAINING_INTRO_OPTION_CORRECT_ID,
      question_id: SEED_E2E_TRAINING_INTRO_QUESTION_ID,
      text: "Administrador",
      is_correct: true,
      sort_order: 0,
      created_at: fixtureCreatedAt,
    },
    {
      id: SEED_E2E_TRAINING_INTRO_OPTION_WRONG_ID,
      question_id: SEED_E2E_TRAINING_INTRO_QUESTION_ID,
      text: "Visitante",
      is_correct: false,
      sort_order: 1,
      created_at: fixtureCreatedAt,
    },
  ]);

  await db.insert(training_enrollments).values({
    id: SEED_E2E_TRAINING_INTRO_ENROLLMENT_ID,
    course_id: SEED_E2E_TRAINING_INTRO_COURSE_ID,
    user_id: adminUserId,
    organization_id: organizationId,
    enrolled_at: fixtureCreatedAt,
  });

  const lectureIds = [
    SEED_E2E_TRAINING_INTRO_LECTURE_1_ID,
    SEED_E2E_TRAINING_INTRO_LECTURE_2_ID,
    SEED_E2E_TRAINING_INTRO_LECTURE_3_ID,
  ];

  await db.insert(training_lecture_progress).values(
    lectureIds.map((lectureId, index) => ({
      id: E2E_INTRO_PROGRESS_IDS[index],
      lecture_id: lectureId,
      user_id: adminUserId,
      organization_id: organizationId,
      status: "completed" as const,
      completed_at: fixtureUpdatedAt,
      updated_at: fixtureUpdatedAt,
    })),
  );

  counters.courses += 2;
  counters.sections += 1;
  counters.modules += 1;
  counters.lectures += 3;
  counters.quizzes += 1;
  counters.questions += 1;
  counters.options += 2;
  counters.enrollments += 1;
  counters.progress += 3;
}

const E2E_TRAINING_COURSE_IDS = [
  SEED_E2E_TRAINING_INTRO_COURSE_ID,
  SEED_E2E_TRAINING_GESTAO_COURSE_ID,
] as const;

/** Idempotent upsert for Playwright — does not reset the rest of the database. */
export async function ensureE2eTrainingFixtures(): Promise<void> {
  await db
    .delete(training_courses)
    .where(inArray(training_courses.id, [...E2E_TRAINING_COURSE_IDS]));

  const counters: SeedCounters = {
    courses: 0,
    sections: 0,
    modules: 0,
    lectures: 0,
    quizzes: 0,
    questions: 0,
    options: 0,
    enrollments: 0,
    progress: 0,
    quizAttempts: 0,
  };

  await seedE2eTrainingFixtures(counters);
}

export async function seedTraining(): Promise<void> {
  const counters: SeedCounters = {
    courses: 0,
    sections: 0,
    modules: 0,
    lectures: 0,
    quizzes: 0,
    questions: 0,
    options: 0,
    enrollments: 0,
    progress: 0,
    quizAttempts: 0,
  };

  ensureBucket();

  await seedE2eTrainingFixtures(counters);

  for (const organizationId of SEED_ORG_IDS) {
    const ownerId = getOrgOwner(organizationId);
    const orgUserIds = getOrgUsers(organizationId);
    const buffers = createBuffers();

    const totalPerOrg = Math.floor(SEED_COUNT / 100);

    for (let i = 0; i < totalPerOrg; i++) {
      const seededCourse = await generateCourse(
        organizationId,
        ownerId,
        buffers,
        counters,
      );

      generateEnrollments(
        seededCourse,
        organizationId,
        orgUserIds,
        buffers,
        counters,
      );

      if ((i + 1) % BATCH_SIZE === 0) {
        await flushIfFull(buffers);
      }
    }

    await flushAllBuffers(buffers);

    Effect.runSync(
      Effect.log(
        `  training (${organizationId}): done (${totalPerOrg} courses)`,
      ),
    );
  }

  Effect.runSync(
    Effect.log(
      `  training: done (${counters.courses} courses, ${counters.sections} sections, ${counters.modules} modules, ${counters.lectures} lectures, ${counters.quizzes} quizzes, ${counters.questions} questions, ${counters.options} options, ${counters.enrollments} enrollments, ${counters.progress} progress rows, ${counters.quizAttempts} quiz attempts)`,
    ),
  );
}
