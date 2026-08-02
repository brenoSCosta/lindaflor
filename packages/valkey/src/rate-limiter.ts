import { valkey } from "@lindaflor/valkey";
import type {
  RatelimiterLimitResult,
  Ratelimiter,
} from "@orpc/experimental-ratelimit";
import { RedisRatelimiter } from "@orpc/experimental-ratelimit/redis";

const RATELIMIT_MAX_REQUESTS = 100;
const RATELIMIT_WINDOW_MS = 60_000;
const RATELIMIT_PREFIX = "ratelimit:";

// Minimal structural deps — same rationale as SecondaryStorageDeps in
// ./secondary-storage.ts: stubs in tests shouldn't have to satisfy iovalkey's
// overloaded signatures.
type RatelimiterDeps = {
  eval(script: string, numKeys: number, ...rest: string[]): Promise<unknown>;
};

// Factory takes a deps object so tests can pass a stub directly — see the
// same pattern (and rationale) in `./secondary-storage.ts`.
export const createRatelimiter = (deps: RatelimiterDeps): Ratelimiter =>
  new RedisRatelimiter({
    eval: (script, numKeys, ...rest) => deps.eval(script, numKeys, ...rest),
    maxRequests: RATELIMIT_MAX_REQUESTS,
    window: RATELIMIT_WINDOW_MS,
    prefix: RATELIMIT_PREFIX,
  });

export const ratelimiter: Ratelimiter = createRatelimiter({
  eval: (script, numKeys, ...rest) => valkey.eval(script, numKeys, ...rest),
});

export const noopRatelimiter: Ratelimiter = {
  limit: (): Promise<RatelimiterLimitResult> =>
    Promise.resolve({
      success: true,
      limit: Number.MAX_SAFE_INTEGER,
      remaining: Number.MAX_SAFE_INTEGER,
      reset: Number.MAX_SAFE_INTEGER,
    }),
};
