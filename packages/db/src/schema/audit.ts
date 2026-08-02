import { organizations, users } from "@lindaflor/db/schema/auth";
import {
  index,
  jsonb,
  pgTable,
  text,
  timestamp,
  uuid,
} from "drizzle-orm/pg-core";
import { v7 as uuidv7 } from "uuid";

export const audit_events = pgTable(
  "audit_events",
  {
    id: uuid("id")
      .primaryKey()
      .$defaultFn(() => uuidv7()),
    organization_id: uuid("organization_id")
      .notNull()
      .references(() => organizations.id, { onDelete: "cascade" }),
    aggregate_type: text("aggregate_type").notNull(),
    aggregate_id: text("aggregate_id").notNull(),
    entity_type: text("entity_type").notNull(),
    entity_id: uuid("entity_id"),
    action: text("action").notNull(),
    actor_user_id: uuid("actor_user_id").references(() => users.id, {
      onDelete: "set null",
    }),
    actor_name: text("actor_name").notNull(),
    metadata: jsonb("metadata"),
    occurred_at: timestamp("occurred_at", {
      withTimezone: true,
      mode: "date",
    }).notNull(),
    created_at: timestamp("created_at").defaultNow().notNull(),
  },
  (table) => [
    index("audit_events_org_aggregate_occurred_at_idx").on(
      table.organization_id,
      table.aggregate_type,
      table.aggregate_id,
      table.occurred_at,
    ),
    index("audit_events_organization_id_idx").on(table.organization_id),
    index("audit_events_entity_type_entity_id_idx").on(
      table.entity_type,
      table.entity_id,
    ),
    index("audit_events_actor_user_id_idx").on(table.actor_user_id),
  ],
);

export type AuditEvent = typeof audit_events.$inferSelect;
export type AuditEventInsert = typeof audit_events.$inferInsert;
