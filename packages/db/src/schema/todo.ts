import { organizations } from "@lindaflor/db/schema/auth";
import { labels, priorities, statuses } from "@lindaflor/shared/enums/todo";
import {
  integer,
  pgEnum,
  pgTable,
  text,
  timestamp,
  uuid,
} from "drizzle-orm/pg-core";

export const statusEnum = pgEnum("status", statuses);

export const labelEnum = pgEnum("label", labels);

export const priorityEnum = pgEnum("priority", priorities);

export const todo = pgTable("todo", {
  id: uuid("id").primaryKey().defaultRandom(),
  text: text("text").notNull(),
  status: statusEnum().default("todo").notNull(),
  label: labelEnum().default("documentation").notNull(),
  priority: priorityEnum().default("medium").notNull(),
  estimated_hours: integer("estimated_hours").default(0),
  actual_hours: integer("actual_hours").default(0),
  progress: integer("progress").default(0),
  cost: integer("cost").default(0),
  due_date: timestamp("due_date"),
  completed_at: timestamp("completed_at"),
  created_at: timestamp("created_at").defaultNow().notNull(),
  updated_at: timestamp("updated_at").defaultNow().notNull(),
  organization_id: uuid("organization_id")
    .notNull()
    .references(() => organizations.id, { onDelete: "cascade" }),
});
export type Todo = typeof todo.$inferSelect;
