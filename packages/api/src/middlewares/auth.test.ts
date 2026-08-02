import { describe, expect, it, mock } from "bun:test";

import type { Context } from "@lindaflor/api/context";
import {
  createBaseContext,
  createSession,
} from "@lindaflor/api/middlewares/test-helpers";
import { call } from "@orpc/server";

void mock.module("@lindaflor/auth", () => ({
  auth: {
    api: {
      hasPermission: mock(() => Promise.resolve(false)),
      userHasPermission: mock(() => Promise.resolve({ success: false })),
    },
  },
  isRole: (s: string): s is "admin" | "user" => s === "admin" || s === "user",
}));

const { protectedProcedure } = await import("@lindaflor/api/middlewares/auth");

describe("protectedProcedure (requireAuth)", () => {
  const procedure = protectedProcedure.handler(() => ({ ok: true }));

  it("throws UNAUTHORIZED when session is null", async () => {
    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: { session: null, ...createBaseContext() },
        }),
      ).rejects.toMatchObject({
        code: "UNAUTHORIZED",
        message: "Você precisa estar autenticado",
      }),
    );
  });

  it("calls handler when session is present", async () => {
    const result = await call(procedure, undefined, {
      context: {
        session: createSession(),
        ...createBaseContext(),
      } satisfies Context,
    });
    expect(result).toEqual({ ok: true });
  });

  it("throws FORBIDDEN when the user is banned", async () => {
    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ banned: true }),
            ...createBaseContext(),
          } satisfies Context,
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Sua conta foi banida",
      }),
    );
  });

  it("does not invoke the handler when session is null", async () => {
    const handler = mock(() => ({ ok: true }));
    const guardedProcedure = protectedProcedure.handler(handler);

    await Promise.resolve(
      expect(
        call(guardedProcedure, undefined, {
          context: { session: null, ...createBaseContext() },
        }),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" }),
    );
    expect(handler).not.toHaveBeenCalled();
  });

  it("does not invoke the handler when the user is banned", async () => {
    const handler = mock(() => ({ ok: true }));
    const guardedProcedure = protectedProcedure.handler(handler);

    await Promise.resolve(
      expect(
        call(guardedProcedure, undefined, {
          context: {
            session: createSession({ banned: true }),
            ...createBaseContext(),
          } satisfies Context,
        }),
      ).rejects.toMatchObject({ code: "FORBIDDEN" }),
    );
    expect(handler).not.toHaveBeenCalled();
  });
});
