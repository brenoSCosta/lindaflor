import { schema } from "@lindaflor/db/schema";
import { env } from "@lindaflor/env/server";
import { drizzle, type PostgresJsDatabase } from "drizzle-orm/postgres-js";
import postgres from "postgres";

export type DbSchema = typeof schema;
export type Database = PostgresJsDatabase<DbSchema>;

function isRemotePostgres(url: string) {
  return (
    url.includes("supabase.com") ||
    url.includes("supabase.co") ||
    (!url.includes("localhost") && !url.includes("127.0.0.1"))
  );
}

export function createPostgresClient(connectionString: string) {
  return postgres(connectionString, {
    // Required for Supabase transaction pooler (port 6543)
    prepare: false,
    ssl: isRemotePostgres(connectionString) ? "require" : undefined,
    max: 10,
    idle_timeout: 20,
    connect_timeout: 10,
  });
}

const runtimeSql = createPostgresClient(env.DATABASE_URL);

export const db: Database = drizzle(runtimeSql, { schema });

export function createMigrationDb() {
  const connectionString = env.DATABASE_URL_DIRECT ?? env.DATABASE_URL;
  const sql = createPostgresClient(connectionString);
  return {
    sql,
    db: drizzle(sql, { schema }),
  };
}
