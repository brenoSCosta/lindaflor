import { afterEach, describe, expect, it, mock } from "bun:test";

import {
  createBaseContext,
  createSession,
} from "@lindaflor/api/middlewares/test-helpers";
import { call, ORPCError } from "@orpc/server";

const userHasPermissionMock = mock(
  (_args: { body: { userId: string; role: string; permissions: unknown } }) =>
    Promise.resolve({ success: false }),
);

void mock.module("@lindaflor/auth", () => ({
  auth: {
    api: {
      hasPermission: mock(() => Promise.resolve(false)),
      userHasPermission: userHasPermissionMock,
    },
  },
  isRole: (s: string): s is "admin" | "moderator" | "user" =>
    s === "admin" || s === "moderator" || s === "user",
}));

const { publicProcedure, o } = await import("@lindaflor/api/middlewares");
const { rbacMiddleware } = await import("@lindaflor/api/middlewares/rbac");
const { auth } = await import("@lindaflor/auth");

describe("rbacMiddleware", () => {
  const procedure = publicProcedure
    .use(rbacMiddleware({ session: ["delete"] }))
    .handler(() => ({ ok: true }));

  afterEach(() => {
    userHasPermissionMock.mockClear();
  });

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

  it("throws FORBIDDEN when all roles return success: false", async () => {
    userHasPermissionMock.mockResolvedValue({ success: false });

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ role: "user" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você não tem permissão para acessar este recurso",
      }),
    );
  });

  it("calls handler when at least one role returns success: true", async () => {
    userHasPermissionMock.mockResolvedValueOnce({ success: true });

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: "user" }),
        ...createBaseContext(),
      },
    });
    expect(result).toEqual({ ok: true });
  });

  it("checks every role when the user has multiple roles", async () => {
    userHasPermissionMock.mockResolvedValue({ success: false });

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ role: "user,admin" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({ code: "FORBIDDEN" }),
    );

    expect(userHasPermissionMock).toHaveBeenCalledTimes(2);
    const calledRoles = userHasPermissionMock.mock.calls.map(
      ([arg]) => arg.body.role,
    );
    expect(calledRoles).toEqual(expect.arrayContaining(["user", "admin"]));
  });

  it("succeeds when a later role grants permission and an earlier role denies", async () => {
    userHasPermissionMock
      .mockResolvedValueOnce({ success: false })
      .mockResolvedValueOnce({ success: true });

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: "user,admin" }),
        ...createBaseContext(),
      },
    });

    expect(result).toEqual({ ok: true });
    expect(userHasPermissionMock).toHaveBeenCalledTimes(2);
  });

  it("fails when no role grants permission across multiple roles", async () => {
    userHasPermissionMock
      .mockResolvedValueOnce({ success: false })
      .mockResolvedValueOnce({ success: false });

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ role: "user,admin" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você não tem permissão para acessar este recurso",
      }),
    );
  });

  // Regression guard: documents why rbac.ts splits the comma-separated role
  // field and checks each role individually. If a future refactor (e.g.
  // migrating better-auth/drizzle from comma-string to a roles array) drops
  // the per-role mapping and passes the raw field once, multi-role users get
  // incorrectly denied — as demonstrated by this inline buggy middleware.
  it("without per-role mapping, multi-role users are incorrectly denied", async () => {
    userHasPermissionMock.mockImplementation(({ body }) =>
      Promise.resolve({ success: body.role === "admin" }),
    );

    const buggyMiddleware = o.middleware(async ({ context, next }) => {
      if (!context.session) {
        throw new ORPCError("UNAUTHORIZED", {
          message: "Você precisa estar autenticado",
        });
      }
      const result = await auth.api.userHasPermission({
        body: {
          userId: context.session.user.id,
          // oxlint-disable-next-line no-unsafe-type-assertion
          role: (context.session.user.role ?? "user") as "admin" | "user",
          permissions: { session: ["delete"] },
        },
      });
      if (!result.success) {
        throw new ORPCError("FORBIDDEN", {
          message: "Você não tem permissão para acessar este recurso",
        });
      }
      return next({ context });
    });

    const buggyProcedure = publicProcedure
      .use(buggyMiddleware)
      .handler(() => ({ ok: true }));

    await Promise.resolve(
      expect(
        call(buggyProcedure, undefined, {
          context: {
            session: createSession({ role: "user,admin" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você não tem permissão para acessar este recurso",
      }),
    );

    expect(userHasPermissionMock).toHaveBeenCalledTimes(1);
    expect(userHasPermissionMock.mock.calls[0]?.[0].body.role).toBe(
      "user,admin",
    );
  });

  it("succeeds when an earlier role grants permission and a later role denies", async () => {
    userHasPermissionMock
      .mockResolvedValueOnce({ success: true })
      .mockResolvedValueOnce({ success: false });

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: "admin,user" }),
        ...createBaseContext(),
      },
    });

    expect(result).toEqual({ ok: true });
    expect(userHasPermissionMock).toHaveBeenCalledTimes(2);
  });

  it("throws FORBIDDEN without calling auth.api when role is an empty string", async () => {
    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ role: "" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você não tem permissão para acessar este recurso",
      }),
    );
    expect(userHasPermissionMock).not.toHaveBeenCalled();
  });

  it("throws FORBIDDEN without calling auth.api when every role is invalid", async () => {
    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ role: "guest,visitor" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você não tem permissão para acessar este recurso",
      }),
    );
    expect(userHasPermissionMock).not.toHaveBeenCalled();
  });

  it("trims whitespace around each role before checking permission", async () => {
    userHasPermissionMock.mockResolvedValue({ success: true });

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: " admin , user " }),
        ...createBaseContext(),
      },
    });

    expect(result).toEqual({ ok: true });
    expect(userHasPermissionMock).toHaveBeenCalledTimes(2);
    const calledRoles = userHasPermissionMock.mock.calls.map(
      ([arg]) => arg.body.role,
    );
    expect(calledRoles).toEqual(expect.arrayContaining(["admin", "user"]));
  });

  it("propagates upstream rejection from auth.api.userHasPermission", async () => {
    const upstreamError = new Error("better-auth unavailable");
    userHasPermissionMock.mockRejectedValueOnce(upstreamError);

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ role: "admin" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toThrow("better-auth unavailable"),
    );
  });
});
