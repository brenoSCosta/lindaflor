import { describe, expect, it, mock } from "bun:test";

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

const { organizationProcedure } =
  await import("@lindaflor/api/middlewares/organization");

describe("organizationProcedure (requireOrganization)", () => {
  const procedure = organizationProcedure.handler(() => ({ ok: true }));

  it("throws UNAUTHORIZED when session has no activeOrganizationId", async () => {
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

  it("calls handler when session has activeOrganizationId", async () => {
    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-1" }),
        ...createBaseContext(),
      },
    });
    expect(result).toEqual({ ok: true });
  });
});
