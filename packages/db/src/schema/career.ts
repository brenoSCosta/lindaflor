import { boolean, pgTable, text, timestamp, uuid } from "drizzle-orm/pg-core";

export const careers = pgTable("career", {
  id: uuid("id").primaryKey().defaultRandom(),
  title: text("title").notNull(),
  department: text("department").notNull(),
  location: text("location").notNull(),
  type: text("type").notNull(),
  description: text("description"),
  requirements: text("requirements").array().notNull().default([]),
  is_active: boolean("is_active").default(true).notNull(),
  created_at: timestamp("created_at").defaultNow().notNull(),
});

export type Career = typeof careers.$inferSelect;
