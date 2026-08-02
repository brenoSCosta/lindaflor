import type { Context } from "@lindaflor/api/context";
import type { Ratelimiter } from "@orpc/experimental-ratelimit";

export const createHeaders = () => new Headers();

export const STUB_CLIENT_IP = "127.0.0.1";

export const stubRatelimiter: Ratelimiter = {
  limit: () =>
    Promise.resolve({
      success: true,
      limit: 100,
      remaining: 99,
      reset: Date.now() + 60_000,
    }),
};

export const createBaseContext = () => ({
  headers: createHeaders(),
  client: { ip: STUB_CLIENT_IP, timezone: "UTC" },
  ratelimiter: stubRatelimiter,
});

export const createSession = (overrides?: {
  activeOrganizationId?: string;
  role?: string;
  banned?: boolean;
}): NonNullable<Context["session"]> =>
  ({
    user: {
      id: "user-1",
      name: "Test User",
      email: "test@example.com",
      emailVerified: true,
      banned: overrides?.banned ?? false,
      createdAt: new Date(),
      updatedAt: new Date(),
      role: overrides?.role ?? "user",
      banReason: null,
      banExpires: null,
      twoFactorEnabled: false,
    },
    session: {
      id: "sess-1",
      createdAt: new Date(),
      updatedAt: new Date(),
      userId: "user-1",
      expiresAt: new Date(Date.now() + 86400000),
      token: "token",
      activeOrganizationId: overrides?.activeOrganizationId ?? "org-1",
    },
  }) satisfies NonNullable<Context["session"]>;
