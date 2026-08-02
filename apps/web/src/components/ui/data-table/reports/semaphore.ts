import { Effect } from "effect";

const POOL_SIZE = Math.min(
  Math.max(1, (navigator.hardwareConcurrency ?? 4) - 1),
  4,
);

const semaphore = Effect.runSync(Effect.makeSemaphore(POOL_SIZE));

export function withReportPermit<A, E, R>(
  effect: Effect.Effect<A, E, R>,
): Effect.Effect<A, E, R> {
  return semaphore.withPermits(1)(effect);
}
