import { describe, expect, it, mock } from "bun:test";

import type { Context } from "@lindaflor/api/context";
import {
  buildRatelimitKey,
  ratelimitMiddleware,
} from "@lindaflor/api/middlewares/rate-limit";
import {
  createBaseContext,
  createSession,
  stubRatelimiter,
} from "@lindaflor/api/middlewares/test-helpers";
import { call, ORPCError } from "@orpc/server";

void mock.module("@lindaflor/auth", () => ({
  auth: {
    api: {
      hasPermission: mock(() => Promise.resolve(false)),
      userHasPermission: mock(() => Promise.resolve({ success: false })),
    },
  },
  isRole: (s: string): s is "admin" | "user" => s === "admin" || s === "user",
}));

const { o } = await import("@lindaflor/api/middlewares");

const allowingRatelimiter = stubRatelimiter;

const denyingRatelimiter = {
  limit: () =>
    Promise.resolve({
      success: false,
      limit: 100,
      remaining: 0,
      reset: Date.now() + 60_000,
    }),
};

describe("buildRatelimitKey", () => {
  it("returns user:<id> when a session is present", () => {
    const context: Context = {
      ...createBaseContext(),
      session: createSession(),
    };
    expect(buildRatelimitKey(context)).toBe("user:user-1");
  });

  it("returns ip:<address> when no session is present", () => {
    const context: Context = {
      ...createBaseContext(),
      session: null,
    };
    expect(buildRatelimitKey(context)).toBe("ip:127.0.0.1");
  });

  it("prefers user over ip when both are available", () => {
    const context: Context = {
      ...createBaseContext(),
      client: { ip: "203.0.113.7", timezone: "UTC" },
      session: createSession(),
    };
    expect(buildRatelimitKey(context)).toBe("user:user-1");
  });
});

describe("ratelimitMiddleware", () => {
  const procedure = o.use(ratelimitMiddleware).handler(() => ({ ok: true }));

  it("passes the handler through when the limiter allows the request", async () => {
    const result = await call(procedure, undefined, {
      context: {
        ...createBaseContext(),
        ratelimiter: allowingRatelimiter,
        session: null,
      },
    });
    expect(result).toEqual({ ok: true });
  });

  it("throws TOO_MANY_REQUESTS when the limiter denies the request", async () => {
    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            ...createBaseContext(),
            ratelimiter: denyingRatelimiter,
            session: null,
          },
        }),
      ).rejects.toBeInstanceOf(ORPCError),
    );
  });
});
