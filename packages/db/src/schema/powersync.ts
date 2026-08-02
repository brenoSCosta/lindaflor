import { organizations, users } from "@lindaflor/db/schema/auth";
import {
  index,
  integer,
  jsonb,
  pgEnum,
  pgTable,
  text,
  timestamp,
  uuid,
} from "drizzle-orm/pg-core";

export const powersync_upload_action_enum = pgEnum("powersync_upload_action", [
  "PUT",
  "PATCH",
  "DELETE",
]);

export const powersync_upload_status_enum = pgEnum("powersync_upload_status", [
  "success",
  "failed",
  "forbidden",
  "invalid",
  "unsupported",
]);

export const powersync_upload_operations = pgTable(
  "powersync_upload_operations",
  {
    id: uuid("id").primaryKey(),
    entity: text("entity").notNull(),
    action: powersync_upload_action_enum().notNull(),
    status: powersync_upload_status_enum().notNull(),
    client_payload: jsonb("client_payload").notNull(),
    server_payload: jsonb("server_payload").notNull(),
    retries_count: integer("retries_count").notNull().default(1),
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
    index("powersync_upload_operations_organization_id_idx").on(
      table.organization_id,
    ),
    index("powersync_upload_operations_updated_at_idx").on(table.updated_at),
  ],
);

export type PowerSyncUploadOperation =
  typeof powersync_upload_operations.$inferSelect;
