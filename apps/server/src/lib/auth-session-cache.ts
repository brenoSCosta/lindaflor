import type { SessionCache } from "@lindaflor/valkey/session-cache";
import { ORPCError } from "@orpc/server";
import { getSessionCookie } from "better-auth/cookies";
import { Effect } from "effect";

const JSON_CONTENT_TYPE = "application/json";

export function extractSessionToken(request: Request): string | null {
  const cookieToken = getSessionCookie(request);
  if (cookieToken) {
    return cookieToken;
  }

  const authorization = request.headers.get("authorization");
  if (authorization?.toLowerCase().startsWith("bearer ")) {
    const token = authorization.slice("bearer ".length).trim();
    return token.length > 0 ? token : null;
  }
  return null;
}

const isCacheableSessionBody = (body: string): boolean => {
  const trimmed = body.trim();
  return trimmed.length > 0 && trimmed !== "null";
};

const getResponseBody = (response: Response) =>
  Effect.tryPromise({
    try: () => response.clone().text(),
    catch: (e) =>
      new ORPCError("BAD_REQUEST", {
        message: "Failed to read auth response stream",
        cause: e,
      }),
  });

type AuthRequestHandlerDeps = {
  authHandler: (request: Request) => Promise<Response>;
  cache: Pick<SessionCache, "read" | "write" | "invalidate">;
  getToken: (request: Request) => string | null;
  ttlSeconds: number;
};

export const createAuthRequestHandler = (deps: AuthRequestHandlerDeps) => {
  const cacheWrite = (token: string, body: string) =>
    Effect.tryPromise({
      try: () =>
        deps.cache.write(token, { status: 200, body }, deps.ttlSeconds),
      catch: (e) =>
        new ORPCError("INTERNAL_SERVER_ERROR", {
          message: "Cache write anomaly",
          cause: e,
        }),
    });

  const cacheRead = (token: string) =>
    Effect.tryPromise({
      try: () => deps.cache.read(token),
      catch: (e) =>
        new ORPCError("INTERNAL_SERVER_ERROR", {
          message: "Cache read anomaly",
          cause: e,
        }),
    });

  const cacheInvalidate = (token: string) =>
    Effect.tryPromise({
      try: () => deps.cache.invalidate(token),
      catch: (e) =>
        new ORPCError("INTERNAL_SERVER_ERROR", {
          message: "Cache invalidation anomaly",
          cause: e,
        }),
    });

  const callAuthHandler = (request: Request) =>
    Effect.tryPromise({
      try: () => deps.authHandler(request),
      catch: (e) =>
        new ORPCError("INTERNAL_SERVER_ERROR", {
          message: "Upstream auth handler exception occurred",
          cause: e,
        }),
    });

  const cacheResponseIfValid = (token: string, response: Response) =>
    Effect.gen(function* () {
      if (response.status !== 200) return;

      const body = yield* getResponseBody(response);
      if (!isCacheableSessionBody(body)) return;

      yield* cacheWrite(token, body);
    }).pipe(
      // Fail Open: Keep catching all errors, log them out, and continue cleanly
      Effect.catchAll((orpcError) =>
        Effect.logWarning(`Cache write skipped: ${orpcError.message}`),
      ),
    );

  return function handleAuthRequest(request: Request): Promise<Response> {
    const program = Effect.gen(function* () {
      const { method } = request;

      if (method !== "GET" && method !== "POST") {
        const error = new ORPCError("BAD_REQUEST", {
          status: 405,
          message: "Only GET and POST methods are permitted on this route",
        });
        yield* Effect.fail(error);
      }

      const url = new URL(request.url);
      const token = deps.getToken(request);
      const isCanonicalGetSession =
        method === "GET" &&
        url.pathname.endsWith("/get-session") &&
        url.search === "";

      if (isCanonicalGetSession && token) {
        const cached = yield* cacheRead(token).pipe(
          // Fail Open: If valkey/redis fails, bypass seamlessly to the origin network call
          Effect.catchAll((orpcError) =>
            Effect.logWarning(`Cache read bypassed: ${orpcError.message}`).pipe(
              Effect.as(null),
            ),
          ),
        );

        if (cached) {
          return new Response(cached.body, {
            status: cached.status,
            headers: { "content-type": JSON_CONTENT_TYPE },
          });
        }

        const response = yield* callAuthHandler(request);
        yield* cacheResponseIfValid(token, response);
        return response;
      }

      const response = yield* callAuthHandler(request);

      if (method !== "GET" && token) {
        yield* cacheInvalidate(token).pipe(
          // Fail Open: Failure to wipe out data won't crash the incoming HTTP pipeline
          Effect.catchAll((orpcError) =>
            Effect.logWarning(
              `Cache invalidation bypassed: ${orpcError.message}`,
            ),
          ),
        );
      }

      return response;
    }).pipe(
      Effect.catchAll((orpcError) => {
        const fallbackStatus = orpcError.status ?? 500;

        return Effect.succeed(
          new Response(
            JSON.stringify({
              error: orpcError.code,
              message: orpcError.message,
            }),
            {
              status: fallbackStatus,
              headers:
                fallbackStatus === 405 ? { Allow: "GET, POST" } : undefined,
            },
          ),
        );
      }),
    );

    return Effect.runPromise(program);
  };
};
