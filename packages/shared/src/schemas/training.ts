import { MAX_FILE_SIZE_BYTES } from "@lindaflor/shared/constants";
import { youtubeUrlSchema } from "@lindaflor/shared/lib/youtube-url";
import { z } from "zod";

const optionalText = z.string().nullish();

export const lectureTypeEnum = z.enum(["video", "pdf", "link"]);
export type LectureType = z.infer<typeof lectureTypeEnum>;

const trainingCourseOutput = z.object({
  id: z.guid(),
  title: z.string(),
  description: optionalText,
  is_published: z.boolean(),
  enrolled: z.boolean(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type TrainingCourseOutput = z.infer<typeof trainingCourseOutput>;

const trainingSectionOutput = z.object({
  id: z.guid(),
  course_id: z.guid(),
  organization_id: z.guid(),
  title: z.string(),
  description: optionalText,
  sort_order: z.number(),
  created_at: z.date(),
});
export type TrainingSectionOutput = z.infer<typeof trainingSectionOutput>;

const trainingModuleOutput = z.object({
  id: z.guid(),
  section_id: z.guid(),
  organization_id: z.guid(),
  title: z.string(),
  description: optionalText,
  sort_order: z.number(),
  created_at: z.date(),
});
export type TrainingModuleOutput = z.infer<typeof trainingModuleOutput>;

const trainingLectureOutput = z.object({
  id: z.guid(),
  module_id: z.guid(),
  organization_id: z.guid(),
  title: z.string(),
  type: lectureTypeEnum,
  youtube_url: optionalText,
  file_key: optionalText,
  file_name: optionalText,
  file_size: z.number().nullable(),
  mime_type: optionalText,
  url: optionalText,
  sort_order: z.number(),
  created_at: z.date(),
});
export type TrainingLectureOutput = z.infer<typeof trainingLectureOutput>;

const trainingQuestionOptionOutput = z.object({
  id: z.guid(),
  question_id: z.guid(),
  text: z.string(),
  is_correct: z.boolean(),
  sort_order: z.number(),
  created_at: z.date(),
});
export type TrainingQuestionOptionOutput = z.infer<
  typeof trainingQuestionOptionOutput
>;

const trainingQuestionOutput = z.object({
  id: z.guid(),
  quiz_id: z.guid(),
  text: z.string(),
  sort_order: z.number(),
  options: z.array(trainingQuestionOptionOutput),
  created_at: z.date(),
});
export type TrainingQuestionOutput = z.infer<typeof trainingQuestionOutput>;

const trainingQuizOutput = z.object({
  id: z.guid(),
  lecture_id: z.guid(),
  organization_id: z.guid(),
  title: z.string(),
  description: optionalText,
  passing_score: z.number(),
  questions: z.array(trainingQuestionOutput),
  created_at: z.date(),
});
export type TrainingQuizOutput = z.infer<typeof trainingQuizOutput>;

const trainingEnrollmentOutput = z.object({
  id: z.guid(),
  course_id: z.guid(),
  user_id: z.guid(),
  organization_id: z.guid(),
  enrolled_at: z.date(),
});
export type TrainingEnrollmentOutput = z.infer<typeof trainingEnrollmentOutput>;

const trainingEnrollmentWithUserOutput = trainingEnrollmentOutput.extend({
  member_id: z.guid(),
  member_role: z.string(),
  user_name: z.string(),
  user_email: z.email(),
  user_image: z.string().nullable(),
});
export type TrainingEnrollmentWithUserOutput = z.infer<
  typeof trainingEnrollmentWithUserOutput
>;

const trainingLectureProgressOutput = z.object({
  id: z.guid(),
  lecture_id: z.guid(),
  user_id: z.guid(),
  organization_id: z.guid(),
  status: z.enum(["not_started", "in_progress", "completed"]),
  completed_at: z.date().nullable(),
  updated_at: z.date(),
});
export type TrainingLectureProgressOutput = z.infer<
  typeof trainingLectureProgressOutput
>;

const trainingQuizAttemptOutput = z.object({
  id: z.guid(),
  quiz_id: z.guid(),
  user_id: z.guid(),
  organization_id: z.guid(),
  score: z.number(),
  passed: z.boolean(),
  answers: z.record(z.string(), z.string()),
  created_at: z.date(),
});
export type TrainingQuizAttemptOutput = z.infer<
  typeof trainingQuizAttemptOutput
>;

const lectureWithChildrenOutput = trainingLectureOutput.extend({
  progress: trainingLectureProgressOutput.nullable(),
  quiz: trainingQuizOutput.nullish().transform((value) => value ?? undefined),
  latest_attempt: trainingQuizAttemptOutput.nullable(),
});

const courseDetailOutput = trainingCourseOutput.extend({
  sections: z.array(
    trainingSectionOutput.extend({
      modules: z.array(
        trainingModuleOutput.extend({
          lectures: z.array(lectureWithChildrenOutput),
        }),
      ),
    }),
  ),
  enrollment: trainingEnrollmentOutput.nullable(),
});
export type TrainingCourseDetailOutput = z.infer<typeof courseDetailOutput>;

export const questionOptionInput = z.object({
  id: z.guid().optional(),
  text: z.string().min(1, "Texto é obrigatório"),
  is_correct: z.boolean(),
  sort_order: z.number(),
});

export const questionInput = z.object({
  id: z.guid().optional(),
  text: z.string().min(1, "Enunciado é obrigatório"),
  sort_order: z.number(),
  options: z.array(questionOptionInput).min(2, "Adicione pelo menos 2 opções"),
});

export const quizInput = z.object({
  id: z.guid().optional(),
  title: z.string().min(1, "Título é obrigatório"),
  description: z.string().optional(),
  passing_score: z.number().min(0).max(100, "Máximo 100"),
  questions: z.array(questionInput).min(1, "Adicione pelo menos 1 pergunta"),
});

export const lectureInput = z
  .object({
    id: z.guid().optional(),
    title: z.string().min(1, "Título é obrigatório"),
    type: lectureTypeEnum,
    youtube_url: youtubeUrlSchema.optional(),
    file_key: z.string().optional(),
    file_name: z.string().optional(),
    file_size: z.number().nullable().optional(),
    mime_type: z.string().optional(),
    url: z.string().optional(),
    sort_order: z.number(),
    quiz: quizInput.optional(),
  })
  .refine((data) => !(data.type === "video" && !data.youtube_url), {
    message: "URL do YouTube é obrigatória para vídeo",
    path: ["youtube_url"],
  })
  .refine(
    (data) =>
      !(data.type === "pdf") ||
      (data.file_key && data.file_name && data.file_size && data.mime_type),
    {
      message: "Arquivo PDF é obrigatório para aula em PDF",
      path: ["file_key"],
    },
  )
  .refine((data) => !(data.type === "link" && !data.url), {
    message: "URL é obrigatória para link",
    path: ["url"],
  });

export const moduleInput = z.object({
  id: z.guid().optional(),
  title: z.string().min(1, "Título é obrigatório"),
  description: z.string().optional(),
  sort_order: z.number(),
  lectures: z.array(lectureInput),
});

export const sectionInput = z.object({
  id: z.guid().optional(),
  title: z.string().min(1, "Título é obrigatório"),
  description: z.string().optional(),
  sort_order: z.number(),
  modules: z.array(moduleInput),
});

export const createCourseInput = z.object({
  title: z.string().min(1, "Título é obrigatório"),
  description: z.string().optional(),
  is_published: z.boolean(),
  sections: z.array(sectionInput),
});

export const updateCourseInput = z.object({
  id: z.guid(),
  title: z.string().min(1, "Título é obrigatório").optional(),
  description: z.string().optional(),
  is_published: z.boolean().optional(),
  sections: z.array(sectionInput),
});

export const listCoursesInput = z.object({
  search: z.string().optional(),
  pageIndex: z.number().default(1),
  pageSize: z.number().default(20),
});

export const listCoursesWithFilterInput = listCoursesInput.extend({
  filter: z.enum(["enrolled", "available"]).optional(),
});

const studentCourseListItemOutput = trainingCourseOutput.extend({
  thumbnail_url: z.string().nullable(),
  is_completed: z.boolean(),
  completed_at: z.date().nullable(),
});
export type StudentCourseListItemOutput = z.infer<
  typeof studentCourseListItemOutput
>;

const courseListOutput = z.object({
  data: z.array(studentCourseListItemOutput),
  meta: z.object({ totalPages: z.number() }),
});

export const schema = {
  v1: {
    courses: {
      list: {
        input: listCoursesWithFilterInput,
        output: courseListOutput,
      },
      get: {
        input: z.object({ id: z.guid() }),
        output: courseDetailOutput,
      },
      create: {
        input: createCourseInput,
        output: courseDetailOutput,
      },
      update: {
        input: updateCourseInput,
        output: courseDetailOutput,
      },
      delete: {
        input: z.object({ id: z.guid() }),
        output: z.null(),
      },
      certificate: {
        get: {
          input: z.object({ id: z.guid() }),
          output: z.object({
            course: trainingCourseOutput,
            completed_at: z.date().nullable(),
            is_completed: z.boolean(),
          }),
        },
      },
    },
    enrollments: {
      list: {
        input: z.object({ course_id: z.guid() }),
        output: z.object({
          data: z.array(trainingEnrollmentWithUserOutput),
        }),
      },
      create: {
        input: z.object({
          course_id: z.guid(),
          user_id: z.guid(),
        }),
        output: trainingEnrollmentOutput,
      },
      delete: {
        input: z.object({
          course_id: z.guid(),
          user_id: z.guid(),
        }),
        output: z.null(),
      },
      selfEnroll: {
        input: z.object({ course_id: z.guid() }),
        output: trainingEnrollmentOutput,
      },
    },
    lectures: {
      pdf: {
        upload: {
          input: z.object({
            file: z
              .file({ error: "Selecione um arquivo PDF" })
              .mime(["application/pdf"], {
                error: "Apenas arquivos PDF são permitidos",
              })
              .max(MAX_FILE_SIZE_BYTES, {
                error: `O arquivo deve ter no máximo ${MAX_FILE_SIZE_BYTES / (1024 * 1024)}MB`,
              }),
          }),
          output: z.object({
            file_key: z.string(),
            file_name: z.string(),
            file_size: z.number(),
            mime_type: z.string(),
          }),
        },
        download: {
          input: z.object({
            lecture_id: z.guid(),
          }),
          output: z.object({
            url: z.url(),
            file_name: z.string().nullable(),
          }),
        },
      },
      progress: {
        mark: {
          input: z.object({
            lecture_id: z.guid(),
            status: z.enum(["not_started", "in_progress", "completed"]),
          }),
          output: trainingLectureProgressOutput,
        },
      },
    },
    quizzes: {
      attempts: {
        submit: {
          input: z.object({
            quiz_id: z.guid(),
            answers: z.record(z.string(), z.string()),
          }),
          output: trainingQuizAttemptOutput,
        },
      },
    },
  },
};
