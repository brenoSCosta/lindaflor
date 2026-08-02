import { cors } from "@elysiajs/cors";
import { createContext } from "@lindaflor/api/context";
import { getClientIp } from "@lindaflor/api/lib/client-ip";
import { appRouter } from "@lindaflor/api/routers";
import { auth } from "@lindaflor/auth";
import { runMigrations } from "@lindaflor/db/migrate";
import { env } from "@lindaflor/env/server";
import { ensureBucket } from "@lindaflor/s3";
import { connectValkey } from "@lindaflor/valkey";
import {
  SESSION_CACHE_TTL_SECONDS,
  sessionCache,
} from "@lindaflor/valkey/session-cache";
import { RatelimitHandlerPlugin } from "@orpc/experimental-ratelimit";
import { OpenAPIHandler } from "@orpc/openapi/fetch";
import { OpenAPIReferencePlugin } from "@orpc/openapi/plugins";
import { onError, ORPCError } from "@orpc/server";
import { RPCHandler } from "@orpc/server/fetch";
import { SimpleCsrfProtectionHandlerPlugin } from "@orpc/server/plugins";
import { ZodToJsonSchemaConverter } from "@orpc/zod/zod4";
import { Cause, Effect, Logger, LogLevel } from "effect";
import { Elysia } from "elysia";

import {
  createAuthRequestHandler,
  extractSessionToken,
} from "@/lib/auth-session-cache";
import {
  BodyLimitPlugin,
  type AppRouterProcedurePath,
  type BodyLimitOptions,
} from "@/lib/body-limit";
import {
  handleDevPaymentConfirm,
  handleMercadoPagoWebhookRequest,
} from "@/webhooks/mercado-pago";
import { buildSitemapXml } from "@/sitemap";
import { releaseExpiredReservations } from "@lindaflor/core/commerce/orders";

const BODY_LIMIT_CONFIG: BodyLimitOptions<AppRouterProcedurePath> = {
  defaultMaxBodySize: 1024 * 1024,
  overrides: {
    "curriculum.v1.submit": 10 * 1024 * 1024,
    "training.v1.lectures.pdf.download": 10 * 1024 * 1024,
    "user.v1.avatar.update": 3 * 1024 * 1024,
    "organization.v1.logo.update": 3 * 1024 * 1024,
    "commerce.admin.uploadProductImage": 5 * 1024 * 1024,
  },
};

const SECURITY_HEADERS: Record<string, string> = {
  "Strict-Transport-Security": "max-age=63072000; includeSubDomains; preload",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "Referrer-Policy": "no-referrer",
  "Cross-Origin-Opener-Policy": "same-origin",
  ...(env.NODE_ENV === "production"
    ? {
        "Content-Security-Policy":
          "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
      }
    : {}),
};

// better-auth resolves the client IP from the leftmost X-Forwarded-For entry,
// which a client can forge to bypass its rate limiter. Pin the header to the
// trusted client IP (resolved from the right) before better-auth reads it.
function withTrustedClientIp(request: Request): Request {
  const ip = getClientIp(request.headers, env.TRUSTED_PROXY_COUNT);
  if (ip === "unknown") {
    return request;
  }
  const headers = new Headers(request.headers);
  headers.set("x-forwarded-for", ip);
  headers.delete("x-real-ip");
  return new Request(request, { headers });
}

const handleAuthRequest = createAuthRequestHandler({
  authHandler: (request) => auth.handler(withTrustedClientIp(request)),
  cache: sessionCache,
  getToken: extractSessionToken,
  ttlSeconds: SESSION_CACHE_TTL_SECONDS,
});

const rpcHandler = new RPCHandler(appRouter, {
  plugins: [
    new BodyLimitPlugin(BODY_LIMIT_CONFIG),
    new RatelimitHandlerPlugin(),
    new SimpleCsrfProtectionHandlerPlugin(),
  ],
  interceptors: [
    onError((error) => {
      Effect.logError(error instanceof ORPCError ? error.message : error);
    }),
  ],
});

const apiHandler = new OpenAPIHandler(appRouter, {
  plugins: [
    ...(env.NODE_ENV !== "production"
      ? [
          new OpenAPIReferencePlugin({
            schemaConverters: [new ZodToJsonSchemaConverter()],
          }),
        ]
      : []),
    new BodyLimitPlugin(BODY_LIMIT_CONFIG),
    new RatelimitHandlerPlugin(),
    new SimpleCsrfProtectionHandlerPlugin(),
  ],
  interceptors: [
    onError((error) => {
      Effect.logError(error instanceof ORPCError ? error.message : error);
    }),
  ],
});

export const app = new Elysia()
  .use(
    cors({
      origin: env.CORS_ORIGINS,
      methods: ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],
      allowedHeaders: ["Content-Type", "Authorization", "x-csrf-token"],
      credentials: true,
    }),
  )
  .onAfterHandle(({ responseValue, set }) => {
    if (responseValue instanceof Response) {
      for (const [key, value] of Object.entries(SECURITY_HEADERS)) {
        responseValue.headers.set(key, value);
      }
    } else {
      Object.assign(set.headers, SECURITY_HEADERS);
    }
  })
  .get("/sitemap.xml", async () => {
    const xml = await buildSitemapXml();
    return new Response(xml, {
      headers: { "Content-Type": "application/xml" },
    });
  })
  .all("/api/auth/*", (context) => handleAuthRequest(context.request))
  .all("/webhooks/mercado-pago", (context) =>
    handleMercadoPagoWebhookRequest(context.request),
  )
  .post("/webhooks/dev/confirm-payment/:orderId", async (context) => {
    if (env.NODE_ENV === "production") {
      return new Response("Not Found", { status: 404 });
    }
    return handleDevPaymentConfirm(context.params.orderId);
  })
  .all("/rpc*", async (context) => {
    const { response } = await rpcHandler.handle(context.request, {
      prefix: "/rpc",
      context: await createContext({ context }),
    });
    return response ?? new Response("Not Found", { status: 404 });
  })
  .all("/api*", async (context) => {
    const { response } = await apiHandler.handle(context.request, {
      prefix: "/api",
      context: await createContext({ context }),
    });
    return response ?? new Response("Not Found", { status: 404 });
  })
  .get("/", () => "OK");

const program = Effect.gen(function* () {
  yield* runMigrations();

  yield* connectValkey();

  if (env.S3_ENDPOINT && env.S3_ACCESS_KEY_ID && env.S3_SECRET_ACCESS_KEY) {
    yield* ensureBucket();
  }

  yield* Effect.log(
    `Server is running on ${env.SERVER_HOST}:${env.SERVER_PORT}`,
  );

  const released = yield* Effect.tryPromise({
    try: () => releaseExpiredReservations(env.ORDER_RESERVATION_HOURS),
    catch: () => new Error("releaseExpiredReservations failed"),
  });
  if (released > 0) {
    yield* Effect.log(`Released ${released} expired order reservations`);
  }

  setInterval(() => {
    void releaseExpiredReservations(env.ORDER_RESERVATION_HOURS);
  }, 60 * 60 * 1000);

  app.listen(env.SERVER_PORT);
});

void Effect.runPromise(
  program.pipe(
    Logger.withMinimumLogLevel(
      env.NODE_ENV === "production" ? LogLevel.Warning : LogLevel.Debug,
    ),
    Effect.catchAllCause((cause) =>
      Effect.sync(() => {
        Effect.runSync(
          Effect.logError("Startup failed: " + Cause.pretty(cause)),
        );
        process.exit(1);
      }),
    ),
  ),
);
