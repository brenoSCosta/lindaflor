import { Effect } from "effect";

import { ensureE2eTrainingFixtures } from "@/seed/seeders/training";

void Effect.runPromise(
  Effect.gen(function* () {
    yield* Effect.log("Ensuring Playwright training fixtures...");
    yield* Effect.tryPromise({
      try: () => ensureE2eTrainingFixtures(),
      catch: (cause) =>
        new Error("ensure-e2e-training-fixtures failed", { cause }),
    });
    yield* Effect.log("Playwright training fixtures ready.");
  }).pipe(
    Effect.tap(() => Effect.sync(() => process.exit(0))),
    Effect.catchAll((error) =>
      Effect.sync(() => {
        const message =
          error instanceof Error
            ? error.cause instanceof Error
              ? error.cause.message
              : error.message
            : String(error);
        console.error(message);
        Effect.runSync(Effect.logError(error));
        process.exit(1);
      }),
    ),
  ),
);
