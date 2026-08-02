import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { db } from "@lindaflor/db";
import { migrate } from "drizzle-orm/node-postgres/migrator";
import { Effect } from "effect";

/**
 * Resolve migrations folder path.
 * In compiled binaries, __dirname points to the binary location.
 * We need to look for migrations relative to the binary or use an absolute path.
 */
function getMigrationsFolder(): Effect.Effect<string, Error> {
  return Effect.gen(function* () {
    // Try multiple possible locations
    const possiblePaths = [
      // Development: relative to source file
      path.join(path.dirname(fileURLToPath(import.meta.url)), "migrations"),
      // Production: relative to binary (if migrations are copied to /app/migrations)
      path.join("/app", "migrations"),
      // Production: relative to binary in same directory
      path.join(process.cwd(), "migrations"),
      // Fallback: try to find from package root
      path.join(process.cwd(), "packages", "db", "src", "migrations"),
    ];

    for (const migrationsPath of possiblePaths) {
      const journalPath = path.join(migrationsPath, "meta", "_journal.json");
      if (existsSync(journalPath)) {
        return migrationsPath;
      }
    }

    // If none found, return the first one and let drizzle error with a clearer message
    const defaultPath = possiblePaths[0] ?? "";
    yield* Effect.logError(
      `[Migration Error] Could not find migrations folder. Tried:\n${possiblePaths.map((p) => `  - ${p}`).join("\n")}`,
    );
    return defaultPath;
  });
}

/**
 * List all migration SQL files in the migrations folder.
 * Excludes files in the meta subdirectory.
 */
function listMigrationFiles(migrationsPath: string): string[] {
  if (!existsSync(migrationsPath)) {
    return [];
  }

  const files: string[] = [];
  const entries = readdirSync(migrationsPath);

  for (const entry of entries) {
    const fullPath = path.join(migrationsPath, entry);
    const stat = statSync(fullPath);

    // Skip meta directory
    if (stat.isDirectory() && entry === "meta") {
      continue;
    }

    // Include only .sql files
    if (stat.isFile() && entry.endsWith(".sql")) {
      files.push(entry);
    }
  }

  // Sort alphabetically to ensure migrations run in order
  return files.toSorted();
}

/**
 * Run pending Drizzle migrations. Use at API startup when the process
 * has access to the DB and to the migrations folder (e.g. in App Runner).
 */
export function runMigrations(): Effect.Effect<void, Error> {
  return Effect.gen(function* () {
    yield* Effect.log("Migrating database...");

    const migrationsFolder = yield* getMigrationsFolder();
    const migrationFiles = listMigrationFiles(migrationsFolder);
    for (const migrationFile of migrationFiles) {
      yield* Effect.log(`Running migration: ${migrationFile}`);
    }

    yield* Effect.tryPromise({
      try: () => migrate(db, { migrationsFolder }),
      catch: (e): Error =>
        e instanceof Error ? e : new Error("Migrations failed"),
    });

    yield* Effect.log("Done!");
  });
}

// CLI entry point
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
