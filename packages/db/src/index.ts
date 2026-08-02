import { schema } from "@lindaflor/db/schema";
import { env } from "@lindaflor/env/server";
import { drizzle, type NodePgDatabase } from "drizzle-orm/node-postgres";

export const db: NodePgDatabase<typeof schema> = drizzle(env.DATABASE_URL, {
  schema,
});
