import { organizations, users } from "@lindaflor/db/schema/auth";
import { relations } from "drizzle-orm";
import {
  boolean,
  index,
  integer,
  jsonb,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core";
import { v7 as uuidv7 } from "uuid";

export const training_courses = pgTable(
  "training_courses",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    title: text("title").notNull(),
    description: text("description"),
    is_published: boolean("is_published").default(false).notNull(),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    created_by_user_id: uuid("created_by_user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("training_courses_organization_id_idx").on(table.organization_id),
    index("training_courses_created_by_user_id_idx").on(
      table.created_by_user_id,
    ),
  ],
);
export type TrainingCourse = typeof training_courses.$inferSelect;

export const training_sections = pgTable(
  "training_sections",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    course_id: uuid("course_id")
      .notNull()
      .references(() => training_courses.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    title: text("title").notNull(),
    description: text("description"),
    sort_order: integer("sort_order").default(0).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("training_sections_course_id_idx").on(table.course_id),
    index("training_sections_organization_id_idx").on(table.organization_id),
  ],
);
export type TrainingSection = typeof training_sections.$inferSelect;

export const training_modules = pgTable(
  "training_modules",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    section_id: uuid("section_id")
      .notNull()
      .references(() => training_sections.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    title: text("title").notNull(),
    description: text("description"),
    sort_order: integer("sort_order").default(0).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("training_modules_section_id_idx").on(table.section_id),
    index("training_modules_organization_id_idx").on(table.organization_id),
  ],
);
export type TrainingModule = typeof training_modules.$inferSelect;

export const training_lectures = pgTable(
  "training_lectures",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    module_id: uuid("module_id")
      .notNull()
      .references(() => training_modules.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    title: text("title").notNull(),
    type: text("type").$type<"video" | "pdf" | "link">().notNull(),
    youtube_url: text("youtube_url"),
    file_key: text("file_key"),
    file_name: text("file_name"),
    file_size: integer("file_size"),
    mime_type: text("mime_type"),
    url: text("url"),
    sort_order: integer("sort_order").default(0).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    index("training_lectures_module_id_idx").on(table.module_id),
    index("training_lectures_organization_id_idx").on(table.organization_id),
  ],
);
export type TrainingLecture = typeof training_lectures.$inferSelect;

export const training_quizzes = pgTable(
  "training_quizzes",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    lecture_id: uuid("lecture_id")
      .notNull()
      .references(() => training_lectures.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    title: text("title").notNull(),
    description: text("description"),
    passing_score: integer("passing_score").notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [
    uniqueIndex("training_quizzes_lecture_id_uidx").on(table.lecture_id),
    index("training_quizzes_organization_id_idx").on(table.organization_id),
  ],
);
export type TrainingQuiz = typeof training_quizzes.$inferSelect;

export const training_questions = pgTable(
  "training_questions",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    quiz_id: uuid("quiz_id")
      .notNull()
      .references(() => training_quizzes.id, { onDelete: "cascade" }),
    text: text("text").notNull(),
    sort_order: integer("sort_order").default(0).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [index("training_questions_quiz_id_idx").on(table.quiz_id)],
);
export type TrainingQuestion = typeof training_questions.$inferSelect;

export const training_question_options = pgTable(
  "training_question_options",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    question_id: uuid("question_id")
      .notNull()
      .references(() => training_questions.id, { onDelete: "cascade" }),
    text: text("text").notNull(),
    is_correct: boolean("is_correct").default(false).notNull(),
    sort_order: integer("sort_order").default(0).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [
    index("training_question_options_question_id_idx").on(table.question_id),
  ],
);
export type TrainingQuestionOption =
  typeof training_question_options.$inferSelect;

export const training_enrollments = pgTable(
  "training_enrollments",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    course_id: uuid("course_id")
      .notNull()
      .references(() => training_courses.id, { onDelete: "cascade" }),
    user_id: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    enrolled_at: timestamp("enrolled_at").defaultNow().notNull(),
  },
  (table) => [
    uniqueIndex("training_enrollments_course_user_uidx").on(
      table.course_id,
      table.user_id,
    ),
    index("training_enrollments_course_id_idx").on(table.course_id),
    index("training_enrollments_user_id_idx").on(table.user_id),
    index("training_enrollments_organization_id_idx").on(table.organization_id),
  ],
);
export type TrainingEnrollment = typeof training_enrollments.$inferSelect;

export const training_lecture_progress = pgTable(
  "training_lecture_progress",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    lecture_id: uuid("lecture_id")
      .notNull()
      .references(() => training_lectures.id, { onDelete: "cascade" }),
    user_id: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    status: text("status")
      .$type<"not_started" | "in_progress" | "completed">()
      .default("not_started")
      .notNull(),
    completed_at: timestamp("completed_at"),
    updated_at: timestamp("updated_at")
      .defaultNow()
      .$onUpdate(() => /* @__PURE__ */ new Date())
      .notNull(),
  },
  (table) => [
    uniqueIndex("training_lecture_progress_lecture_user_uidx").on(
      table.lecture_id,
      table.user_id,
    ),
    index("training_lecture_progress_lecture_id_idx").on(table.lecture_id),
    index("training_lecture_progress_user_id_idx").on(table.user_id),
    index("training_lecture_progress_organization_id_idx").on(
      table.organization_id,
    ),
  ],
);
export type TrainingLectureProgress =
  typeof training_lecture_progress.$inferSelect;

export const training_quiz_attempts = pgTable(
  "training_quiz_attempts",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    quiz_id: uuid("quiz_id")
      .notNull()
      .references(() => training_quizzes.id, { onDelete: "cascade" }),
    user_id: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    score: integer("score").notNull(),
    passed: boolean("passed").default(false).notNull(),
    answers: jsonb("answers").notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [
    index("training_quiz_attempts_quiz_id_idx").on(table.quiz_id),
    index("training_quiz_attempts_user_id_idx").on(table.user_id),
    index("training_quiz_attempts_organization_id_idx").on(
      table.organization_id,
    ),
  ],
);
export type TrainingQuizAttempt = typeof training_quiz_attempts.$inferSelect;

export const training_course_relations = relations(
  training_courses,
  ({ many, one }) => ({
    organization: one(organizations, {
      fields: [training_courses.organization_id],
      references: [organizations.id],
    }),
    created_by: one(users, {
      fields: [training_courses.created_by_user_id],
      references: [users.id],
    }),
    sections: many(training_sections),
    enrollments: many(training_enrollments),
  }),
);

export const training_section_relations = relations(
  training_sections,
  ({ one, many }) => ({
    course: one(training_courses, {
      fields: [training_sections.course_id],
      references: [training_courses.id],
    }),
    organization: one(organizations, {
      fields: [training_sections.organization_id],
      references: [organizations.id],
    }),
    modules: many(training_modules),
  }),
);

export const training_module_relations = relations(
  training_modules,
  ({ one, many }) => ({
    section: one(training_sections, {
      fields: [training_modules.section_id],
      references: [training_sections.id],
    }),
    organization: one(organizations, {
      fields: [training_modules.organization_id],
      references: [organizations.id],
    }),
    lectures: many(training_lectures),
  }),
);

export const training_lecture_relations = relations(
  training_lectures,
  ({ one, many }) => ({
    module: one(training_modules, {
      fields: [training_lectures.module_id],
      references: [training_modules.id],
    }),
    organization: one(organizations, {
      fields: [training_lectures.organization_id],
      references: [organizations.id],
    }),
    quiz: one(training_quizzes),
    progress: many(training_lecture_progress),
  }),
);

export const training_quiz_relations = relations(
  training_quizzes,
  ({ one, many }) => ({
    lecture: one(training_lectures, {
      fields: [training_quizzes.lecture_id],
      references: [training_lectures.id],
    }),
    organization: one(organizations, {
      fields: [training_quizzes.organization_id],
      references: [organizations.id],
    }),
    questions: many(training_questions),
    attempts: many(training_quiz_attempts),
  }),
);

export const training_question_relations = relations(
  training_questions,
  ({ one, many }) => ({
    quiz: one(training_quizzes, {
      fields: [training_questions.quiz_id],
      references: [training_quizzes.id],
    }),
    options: many(training_question_options),
  }),
);

export const training_question_option_relations = relations(
  training_question_options,
  ({ one }) => ({
    question: one(training_questions, {
      fields: [training_question_options.question_id],
      references: [training_questions.id],
    }),
  }),
);

export const training_enrollment_relations = relations(
  training_enrollments,
  ({ one }) => ({
    course: one(training_courses, {
      fields: [training_enrollments.course_id],
      references: [training_courses.id],
    }),
    user: one(users, {
      fields: [training_enrollments.user_id],
      references: [users.id],
    }),
    organization: one(organizations, {
      fields: [training_enrollments.organization_id],
      references: [organizations.id],
    }),
  }),
);

export const training_lecture_progress_relations = relations(
  training_lecture_progress,
  ({ one }) => ({
    lecture: one(training_lectures, {
      fields: [training_lecture_progress.lecture_id],
      references: [training_lectures.id],
    }),
    user: one(users, {
      fields: [training_lecture_progress.user_id],
      references: [users.id],
    }),
    organization: one(organizations, {
      fields: [training_lecture_progress.organization_id],
      references: [organizations.id],
    }),
  }),
);

export const training_quiz_attempt_relations = relations(
  training_quiz_attempts,
  ({ one }) => ({
    quiz: one(training_quizzes, {
      fields: [training_quiz_attempts.quiz_id],
      references: [training_quizzes.id],
    }),
    user: one(users, {
      fields: [training_quiz_attempts.user_id],
      references: [users.id],
    }),
    organization: one(organizations, {
      fields: [training_quiz_attempts.organization_id],
      references: [organizations.id],
    }),
  }),
);
