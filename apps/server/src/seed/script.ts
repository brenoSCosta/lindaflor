import { auth } from "@lindaflor/auth";
import { db } from "@lindaflor/db";
import { schema } from "@lindaflor/db/schema";
import { organizations } from "@lindaflor/db/schema/auth";
import { env } from "@lindaflor/env/server";
import { reset } from "drizzle-seed";
import { Effect, Logger, LogLevel } from "effect";

import { DEV_USERS, SEED_ORGANIZATIONS } from "@/seed/constants";
import { seedDevUsersWithAuth } from "@/seed/seeders/auth";
import { seedCommerce } from "@/seed/seeders/commerce";

function main(): Effect.Effect<void, Error> {
  return Effect.gen(function* () {
    if (env.NODE_ENV !== "development") {
      yield* Effect.logError(
        "seed: Skipping. Run only when NODE_ENV=development",
      );
      process.exit(1);
    }

    const plainPassword = env.SEED_DEV_PASSWORD;

    if (!plainPassword) {
      yield* Effect.logError("SEED_DEV_PASSWORD is not set");
      process.exit(1);
    }

    yield* Effect.log("Starting Linda Flor database seeding...");

    yield* Effect.tryPromise({
      try: () => reset(db, schema),
      catch: () => new Error("seed step 'reset' failed"),
    });

    yield* Effect.tryPromise({
      try: () => db.insert(organizations).values(SEED_ORGANIZATIONS),
      catch: () => new Error("seed step 'insert organizations' failed"),
    });

    const ctx = yield* Effect.tryPromise({
      try: () => auth.$context,
      catch: () => new Error("seed step 'resolve auth context' failed"),
    });

    const hashedPassword = yield* Effect.tryPromise({
      try: () => ctx.password.hash(plainPassword),
      catch: () => new Error("seed step 'hash password' failed"),
    });

    yield* Effect.tryPromise({
      try: () => seedDevUsersWithAuth(hashedPassword),
      catch: () => new Error("seed step 'seed dev users' failed"),
    });

    yield* Effect.tryPromise({
      try: () => seedCommerce(),
      catch: () => new Error("seed step 'seed commerce' failed"),
    });

    yield* Effect.log("Seed completed.");
    yield* Effect.log(`Admin login: ${DEV_USERS[0]?.email} / ${plainPassword}`);
  });
}

void Effect.runPromise(
  main().pipe(
    Logger.withMinimumLogLevel(LogLevel.Debug),
    Effect.tap(() => Effect.sync(() => process.exit(0))),
    Effect.catchAll((error) =>
      Effect.sync(() => {
        Effect.runSync(Effect.logError("Seed failed:", error));
        process.exit(1);
      }),
    ),
  ),
);
