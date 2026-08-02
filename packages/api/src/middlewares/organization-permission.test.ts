import { afterEach, describe, expect, it, mock } from "bun:test";

import {
  createBaseContext,
  createSession,
} from "@lindaflor/api/middlewares/test-helpers";
import { call } from "@orpc/server";

const hasPermissionMock = mock(() => Promise.resolve(false));

void mock.module("@lindaflor/auth", () => ({
  auth: {
    api: {
      hasPermission: hasPermissionMock,
      userHasPermission: mock(() => Promise.resolve({ success: false })),
    },
  },
  isRole: (s: string): s is "admin" | "user" => s === "admin" || s === "user",
}));

const { publicProcedure } = await import("@lindaflor/api/middlewares");
const { organizationMiddleware } =
  await import("@lindaflor/api/middlewares/organization-permission");

describe("organizationMiddleware", () => {
  const procedure = publicProcedure
    .use(organizationMiddleware({ organization: ["delete"] }))
    .handler(() => ({ ok: true }));

  afterEach(() => {
    hasPermissionMock.mockClear();
  });

  it("throws UNAUTHORIZED when session is null", async () => {
    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: { session: null, ...createBaseContext() },
        }),
      ).rejects.toMatchObject({
        code: "UNAUTHORIZED",
        message: "Você precisa estar em uma organização",
      }),
    );
  });

  it("throws UNAUTHORIZED when activeOrganizationId is missing", async () => {
    const session = createSession();
    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: {
              ...session,
              session: {
                ...session.session,
                activeOrganizationId: undefined,
              },
            },
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "UNAUTHORIZED",
        message: "Você precisa estar em uma organização",
      }),
    );
  });

  it("throws FORBIDDEN when hasPermission returns false", async () => {
    hasPermissionMock.mockResolvedValue(false);

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession(),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você não tem permissão para acessar este recurso",
      }),
    );
  });

  it("calls handler when hasPermission returns true", async () => {
    hasPermissionMock.mockResolvedValue(true);

    const result = await call(procedure, undefined, {
      context: {
        session: createSession(),
        ...createBaseContext(),
      },
    });
    expect(result).toEqual({ ok: true });
  });
});
