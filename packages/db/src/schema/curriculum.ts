import {
  integer,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core";

export const curriculums = pgTable(
  "curriculum",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    name: text("name").notNull(),
    email: text("email").notNull(),
    phone: text("phone"),
    headline: text("headline").notNull(),
    summary: text("summary"),
    skills: text("skills").array().notNull().default([]),
    career_id: uuid("career_id"),
    file_key: text("file_key").notNull(),
    file_name: text("file_name").notNull(),
    file_size: integer("file_size").notNull(),
    mime_type: text("mime_type").notNull().default("application/pdf"),
    submitted_at: timestamp("submitted_at").defaultNow().notNull(),
  },
  (table) => [uniqueIndex("curriculum_file_key_idx").on(table.file_key)],
);

export type Curriculum = typeof curriculums.$inferSelect;
