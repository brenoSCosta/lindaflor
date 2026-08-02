import { createHash } from "node:crypto";

import { valkey } from "@lindaflor/valkey";
import { Effect } from "effect";

export const SESSION_CACHE_TTL_SECONDS = 60;
export const SESSION_CACHE_PREFIX = "auth:session-cache:v1:";

export type CachedSessionResponse = {
  status: number;
  body: string;
};

type SessionCacheDeps = {
  get(key: string): Promise<string | null | undefined>;
  set(key: string, value: string, ttlSeconds: number): Promise<unknown>;
  del(key: string): Promise<unknown>;
};

export type SessionCache = {
  cacheKey(token: string): string;
  read(token: string): Promise<CachedSessionResponse | null>;
  write(
    token: string,
    response: CachedSessionResponse,
    ttlSeconds?: number,
  ): Promise<void>;
  invalidate(token: string): Promise<void>;
};

function isCachedSessionResponse(
  value: unknown,
): value is CachedSessionResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    "status" in value &&
    typeof value.status === "number" &&
    "body" in value &&
    typeof value.body === "string"
  );
}

export const createSessionCache = (deps: SessionCacheDeps): SessionCache => {
  const cacheKey = (token: string): string =>
    `${SESSION_CACHE_PREFIX}${createHash("sha256").update(token).digest("hex")}`;

  return {
    cacheKey,
    read: async (token) => {
      const raw = await deps.get(cacheKey(token));
      if (!raw) {
        return null;
      }
      return Effect.runSync(
        Effect.try({
          try: () => {
            const parsed: unknown = JSON.parse(raw);
            return isCachedSessionResponse(parsed) ? parsed : null;
          },
          catch: () => new Error("invalid JSON in cache"),
        }).pipe(Effect.catchAll(() => Effect.succeed(null))),
      );
    },
    write: async (token, response, ttlSeconds = SESSION_CACHE_TTL_SECONDS) => {
      await deps.set(cacheKey(token), JSON.stringify(response), ttlSeconds);
    },
    invalidate: async (token) => {
      await deps.del(cacheKey(token));
    },
  };
};

export const sessionCache: SessionCache = createSessionCache({
  get: (key) => valkey.get(key),
  set: (key, value, ttlSeconds) => valkey.set(key, value, "EX", ttlSeconds),
  del: (key) => valkey.del(key),
});
