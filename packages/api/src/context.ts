import { getClientIp } from "@lindaflor/api/lib/client-ip";
import { auth } from "@lindaflor/auth";
import { env } from "@lindaflor/env/server";
import { CLIENT_TIMEZONE_COOKIE_NAME } from "@lindaflor/shared/constants";
import { ianaTimeZoneSchema } from "@lindaflor/shared/lib/date-filter";
import { noopRatelimiter, ratelimiter } from "@lindaflor/valkey/rate-limiter";
import type { Ratelimiter } from "@orpc/experimental-ratelimit";
import { getCookie } from "@orpc/server/helpers";
import type { Context as ElysiaContext } from "elysia";
import z from "zod";

type ContextOptions = {
  context: ElysiaContext;
};

const DEFAULT_TIMEZONE = "UTC";

const client = z.object({
  ip: z.ipv4().or(z.ipv6()),
  timezone: ianaTimeZoneSchema,
});
type Client = z.infer<typeof client>;

type ContextPromise = {
  session: Awaited<ReturnType<typeof auth.api.getSession>>;
  headers: Headers;
  client: Client;
  ratelimiter: Ratelimiter;
};

export async function createContext({
  context,
}: ContextOptions): Promise<ContextPromise> {
  const session = await auth.api.getSession({
    headers: context.request.headers,
  });

  const timezoneCookie = getCookie(
    context.request.headers,
    CLIENT_TIMEZONE_COOKIE_NAME,
  );

  return {
    session,
    headers: context.request.headers,
    client: {
      ip: getClientIp(context.request.headers, env.TRUSTED_PROXY_COUNT),
      timezone: timezoneCookie?.trim() || DEFAULT_TIMEZONE,
    },
    ratelimiter: env.NODE_ENV === "production" ? ratelimiter : noopRatelimiter,
  };
}

export type Context = Awaited<ReturnType<typeof createContext>>;
