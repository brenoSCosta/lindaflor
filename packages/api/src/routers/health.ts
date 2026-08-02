import type { Context } from "@lindaflor/api/context";
import { o, publicProcedure } from "@lindaflor/api/middlewares";
import {
  healthCheckV1,
  healthCheckV2,
} from "@lindaflor/core/health/health-check";
import type { EnhancedRouter } from "@orpc/server";
import { z } from "zod";

const healthV1Routes = {
  healthCheck: publicProcedure
    .output(z.literal("OK"))
    .route({
      method: "GET",
      path: "/health",
    })
    .handler(() => healthCheckV1()),
};

const healthV2Routes = {
  healthCheck: publicProcedure
    .route({
      method: "GET",
      path: "/health",
    })
    .output(
      z.object({
        status: z.literal("OK"),
        date: z.string(),
        timezone: z.string(),
        timezoneOffset: z.number(),
        timezoneName: z.string(),
        version: z.string().default("2.0.0"),
        uptime: z.number(),
        memory: z.object({
          rss: z.number(),
          heapTotal: z.number(),
          heapUsed: z.number(),
          external: z.number(),
        }),
      }),
    )
    .handler(() => healthCheckV2()),
};

type HealthV1Routes = typeof healthV1Routes;
type HealthV2Routes = typeof healthV2Routes;

type HealthV1Router = EnhancedRouter<
  HealthV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type HealthV2Router = EnhancedRouter<
  HealthV2Routes,
  Context,
  Context,
  Record<never, never>
>;

type HealthRouter = {
  v1: HealthV1Router;
  v2: HealthV2Router;
};

function createHealthV1Router(routes: HealthV1Routes): HealthV1Router {
  return o.prefix("/v1").tag("Health").router(routes);
}

function createHealthV2Router(routes: HealthV2Routes): HealthV2Router {
  return o.prefix("/v2").tag("Health").router(routes);
}

export const healthRouter: HealthRouter = {
  v1: createHealthV1Router(healthV1Routes),
  v2: createHealthV2Router(healthV2Routes),
};
