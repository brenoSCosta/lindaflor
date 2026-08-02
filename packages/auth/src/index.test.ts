import { describe, expect, it, mock } from "bun:test";

void mock.module("@lindaflor/db", () => ({ db: {} }));

void mock.module("@lindaflor/db/schema/auth", () => ({
  accounts: {},
  invitations: {},
  jwkss: {},
  members: {},
  organizations: {},
  sessions: {},
  two_factors: {},
  users: {},
  verifications: {},
}));

void mock.module("better-auth/plugins", () => ({
  admin: () => () => ({}),
  bearer: () => () => ({}),
  haveIBeenPwned: () => () => ({}),
  organization: () => () => ({}),
  twoFactor: () => () => ({}),
}));

void mock.module("@lindaflor/env/server", () => ({
  env: {
    WEB_ORIGIN: "http://localhost:9001",
    CORS_ORIGINS: ["http://localhost:9001", "http://localhost:3000"],
    BETTER_AUTH_SECRET: "x".repeat(32),
    BETTER_AUTH_URL: "http://localhost:9000",
    DATABASE_URL: "postgres://localhost/test",
    VALKEY_URL: "redis://localhost:6379",
    NODE_ENV: "test",
    SERVER_PORT: 9000,
    SERVER_HOST: "0.0.0.0",
    RESEND_API_KEY: "re_test",
    MAIL_FROM: "test@example.com",
    PS_URL: "http://localhost:8080",
  },
}));

void mock.module("@lindaflor/mail/templates", () => ({
  sendOrganizationCreatedEmail: () => Promise.resolve(),
}));

// Mock the base Valkey client rather than the secondary-storage module so the
// real `@lindaflor/valkey/secondary-storage` exports stay intact. Mocking
// secondary-storage directly previously masked its `createValkeySecondaryStorage`
// factory, which broke unrelated tests that import the factory.
void mock.module("@lindaflor/valkey", () => ({
  valkey: {
    get: () => Promise.resolve(null),
    set: () => Promise.resolve("OK"),
    del: () => Promise.resolve(1),
    eval: () => Promise.resolve([1, 100, 99, Date.now() + 60_000]),
  },
}));

const { env } = await import("@lindaflor/env/server");

const { auth } = await import("@lindaflor/auth");

describe("auth.options.rateLimit", () => {
  it("is enabled in production only", () => {
    expect(auth.options.rateLimit?.enabled).toBe(env.NODE_ENV === "production");
  });

  it("uses Valkey-backed secondary storage", () => {
    expect(auth.options.rateLimit?.storage).toBe("secondary-storage");
  });

  it("has a global baseline of 100 requests per 60 seconds", () => {
    expect(auth.options.rateLimit?.window).toBe(60);
    expect(auth.options.rateLimit?.max).toBe(100);
  });

  describe("customRules", () => {
    it("declares the /sign-in/email rule", () => {
      const rule = auth.options.rateLimit?.customRules?.["/sign-in/email"];
      expect(rule).toBeDefined();
    });

    it("locks /sign-in/email to 3 attempts per 10 seconds", () => {
      const rule = auth.options.rateLimit?.customRules?.["/sign-in/email"];
      expect(rule).toEqual({ window: 10, max: 3 });
    });

    it("locks /two-factor/verify-totp to 5 attempts per 10 seconds", () => {
      const rule =
        auth.options.rateLimit?.customRules?.["/two-factor/verify-totp"];
      expect(rule).toEqual({ window: 10, max: 5 });
    });

    it("locks /two-factor/enable to 5 attempts per 60 seconds", () => {
      const rule = auth.options.rateLimit?.customRules?.["/two-factor/enable"];
      expect(rule).toEqual({ window: 60, max: 5 });
    });
  });
});
