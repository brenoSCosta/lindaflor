import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { createMigrationDb } from "@lindaflor/db/client";
import { migrate } from "drizzle-orm/postgres-js/migrator";
import { Effect } from "effect";

function getMigrationsFolder(): Effect.Effect<string, Error> {
  return Effect.gen(function* () {
    const possiblePaths = [
      path.join(path.dirname(fileURLToPath(import.meta.url)), "migrations"),
      path.join("/app", "migrations"),
      path.join(process.cwd(), "migrations"),
      path.join(process.cwd(), "packages", "db", "src", "migrations"),
    ];

    for (const migrationsPath of possiblePaths) {
      const journalPath = path.join(migrationsPath, "meta", "_journal.json");
      if (existsSync(journalPath)) {
        return migrationsPath;
      }
    }

    const defaultPath = possiblePaths[0] ?? "";
    yield* Effect.logError(
      `[Migration Error] Could not find migrations folder. Tried:\n${possiblePaths.map((p) => `  - ${p}`).join("\n")}`,
    );
    return defaultPath;
  });
}

function listMigrationFiles(migrationsPath: string): string[] {
  if (!existsSync(migrationsPath)) {
    return [];
  }

  const files: string[] = [];
  const entries = readdirSync(migrationsPath);

  for (const entry of entries) {
    const fullPath = path.join(migrationsPath, entry);
    const stat = statSync(fullPath);

    if (stat.isDirectory() && entry === "meta") {
      continue;
    }

    if (stat.isFile() && entry.endsWith(".sql")) {
      files.push(entry);
    }
  }

  return files.toSorted();
}

export function runMigrations(): Effect.Effect<void, Error> {
  return Effect.gen(function* () {
    yield* Effect.log("Migrating database...");

    const migrationsFolder = yield* getMigrationsFolder();
    const migrationFiles = listMigrationFiles(migrationsFolder);
    for (const migrationFile of migrationFiles) {
      yield* Effect.log(`Running migration: ${migrationFile}`);
    }

    const { db, sql } = createMigrationDb();

    yield* Effect.tryPromise({
      try: async () => {
        await migrate(db, { migrationsFolder });
        await sql.end({ timeout: 5 });
      },
      catch: (e): Error =>
        e instanceof Error ? e : new Error("Migrations failed"),
    });

    yield* Effect.log("Done!");
  });
}

if (import.meta.main) {
  void Effect.runPromise(
    runMigrations().pipe(
      Effect.tap(() => Effect.sync(() => process.exit(0))),
      Effect.catchAll((error) =>
        Effect.sync(() => {
          Effect.runSync(Effect.logError("Migrations failed:", error));
          process.exit(1);
        }),
      ),
    ),
  );
}
