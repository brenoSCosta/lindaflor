import { describe, expect, it, mock } from "bun:test";

import {
  createBaseContext,
  createSession,
} from "@lindaflor/api/middlewares/test-helpers";
import { defineAbilityFor } from "@lindaflor/shared/lib/ability";
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

void mock.module("@lindaflor/db", () => ({
  db: {
    select: () => ({
      from: () => ({
        where: () => ({ limit: () => Promise.resolve([]) }),
      }),
    }),
  },
}));

const { oAuthorized } = await import("@lindaflor/api/middlewares/authorized");
const { requireActiveOrganization } =
  await import("@lindaflor/api/middlewares/require-active-organization");

const ability = defineAbilityFor({
  userId: "user-1",
  roles: ["user"],
  activeOrganizationId: null,
  orgRole: null,
});

describe("requireActiveOrganization", () => {
  it("throws FORBIDDEN when activeOrganizationId is missing", async () => {
    const procedure = oAuthorized
      .use(requireActiveOrganization())
      .handler(() => ({ ok: true }));

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
            ability,
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você precisa estar em uma organização",
      }),
    );
  });

  it("exposes activeOrganizationId on context when present", async () => {
    const procedure = oAuthorized
      .use(requireActiveOrganization())
      .handler(({ context }) => ({
        activeOrganizationId: context.activeOrganizationId,
      }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-7" }),
        ...createBaseContext(),
        ability,
      },
    });
    expect(result).toEqual({ activeOrganizationId: "org-7" });
  });
});
