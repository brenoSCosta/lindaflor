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
const { authorize } = await import("@lindaflor/api/middlewares/authorize");

describe("authorize", () => {
  it("throws FORBIDDEN when ability cannot perform action", async () => {
    const procedure = oAuthorized
      .use(authorize("delete", "Todo"))
      .handler(() => ({ ok: true }));

    const ability = defineAbilityFor({
      userId: "user-1",
      roles: ["user"],
      activeOrganizationId: "org-1",
      orgRole: "member",
    });

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ activeOrganizationId: "org-1" }),
            ...createBaseContext(),
            ability,
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Você não tem permissão para executar esta ação",
      }),
    );
  });

  it("calls handler when ability can perform action", async () => {
    const procedure = oAuthorized
      .use(authorize("create", "Todo"))
      .handler(() => ({ ok: true }));

    const ability = defineAbilityFor({
      userId: "user-1",
      roles: ["user"],
      activeOrganizationId: "org-1",
      orgRole: "owner",
    });

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-1" }),
        ...createBaseContext(),
        ability,
      },
    });
    expect(result).toEqual({ ok: true });
  });

  it("honors admin RBAC ability regardless of org role", async () => {
    const procedure = oAuthorized
      .use(authorize("ban", "User"))
      .handler(() => ({ ok: true }));

    const ability = defineAbilityFor({
      userId: "admin-1",
      roles: ["admin"],
      activeOrganizationId: null,
      orgRole: null,
    });

    const result = await call(procedure, undefined, {
      context: {
        session: createSession(),
        ...createBaseContext(),
        ability,
      },
    });
    expect(result).toEqual({ ok: true });
  });
});
