import type { Context } from "@lindaflor/api/context";
import { createRatelimitMiddleware } from "@orpc/experimental-ratelimit";

export const buildRatelimitKey = (context: Context): string => {
  const userId = context.session?.user.id;
  if (userId) {
    return `user:${userId}`;
  }
  return `ip:${context.client.ip}`;
};

export const ratelimitMiddleware = createRatelimitMiddleware<Context>({
  limiter: ({ context }) => context.ratelimiter,
  key: ({ context }) => buildRatelimitKey(context),
});
