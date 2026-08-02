import { beforeEach, describe, expect, it, mock } from "bun:test";

import type { Context } from "@lindaflor/api/context";
import {
  createBaseContext,
  createSession,
} from "@lindaflor/api/middlewares/test-helpers";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import { call } from "@orpc/server";

const memberQueryMock = mock(() => Promise.resolve([] as { role: string }[]));

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
        where: () => ({ limit: memberQueryMock }),
      }),
    }),
  },
}));

const { authorizedProcedure } =
  await import("@lindaflor/api/middlewares/authorized");

const sessionWithoutOrg = () => {
  const session = createSession();
  return {
    ...session,
    session: {
      ...session.session,
      activeOrganizationId: undefined,
    },
  } as NonNullable<Context["session"]>;
};

describe("authorizedProcedure", () => {
  beforeEach(() => {
    memberQueryMock.mockReset();
    memberQueryMock.mockImplementation(() => Promise.resolve([]));
  });

  it("throws UNAUTHORIZED when session is null", async () => {
    const procedure = authorizedProcedure.handler(() => ({ ok: true }));

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

  it("skips the member query when there is no active organization", async () => {
    const procedure = authorizedProcedure.handler(() => ({ ok: true }));

    await call(procedure, undefined, {
      context: {
        session: sessionWithoutOrg(),
        ...createBaseContext(),
      },
    });
    expect(memberQueryMock).not.toHaveBeenCalled();
  });

  it("grants admin User-management ability when user has admin RBAC role", async () => {
    const procedure = authorizedProcedure.handler(({ context }) => ({
      canManageUser: context.ability.can("manage", "User"),
      canReadTodo: context.ability.can("read", "Todo"),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: "admin" }),
        ...createBaseContext(),
      },
    });
    expect(result.canManageUser).toBe(true);
    expect(result.canReadTodo).toBe(false);
  });

  it("combines admin User-management with org-owner Todo access when admin owns the active org", async () => {
    memberQueryMock.mockResolvedValueOnce([{ role: "owner" }]);

    const procedure = authorizedProcedure.handler(({ context }) => ({
      canManageUser: context.ability.can("manage", "User"),
      canDeleteTodo: context.ability.can(
        "delete",
        subject("Todo", {
          id: "t-1",
          text: "t",
          status: "todo",
          label: "documentation",
          priority: "medium",
          estimated_hours: 0,
          actual_hours: 0,
          progress: 0,
          cost: 0,
          due_date: null,
          completed_at: null,
          created_at: new Date(),
          updated_at: new Date(),
          organization_id: "org-1",
        }),
      ),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({
          role: "admin",
          activeOrganizationId: "org-1",
        }),
        ...createBaseContext(),
      },
    });
    expect(result.canManageUser).toBe(true);
    expect(result.canDeleteTodo).toBe(true);
  });

  it("applies owner permissions when the member row says owner", async () => {
    memberQueryMock.mockResolvedValueOnce([{ role: "owner" }]);

    const procedure = authorizedProcedure.handler(({ context }) => ({
      canDeleteMember: context.ability.can(
        "delete",
        subject("Member", {
          id: "m-1",
          organization_id: "org-1",
          user_id: "other",
          role: "member",
          created_at: new Date(),
        }),
      ),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-1" }),
        ...createBaseContext(),
      },
    });
    expect(result.canDeleteMember).toBe(true);
  });

  it("ignores unknown member roles", async () => {
    memberQueryMock.mockResolvedValueOnce([{ role: "guest" }]);

    const procedure = authorizedProcedure.handler(({ context }) => ({
      canReadTodo: context.ability.can("read", "Todo"),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-1" }),
        ...createBaseContext(),
      },
    });
    expect(result.canReadTodo).toBe(false);
  });

  it("blocks a banned user before computing ability", async () => {
    const procedure = authorizedProcedure.handler(() => ({ ok: true }));

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ role: "admin", banned: true }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toMatchObject({
        code: "FORBIDDEN",
        message: "Sua conta foi banida",
      }),
    );

    expect(memberQueryMock).not.toHaveBeenCalled();
  });

  it("leaves orgRole null when no member row exists", async () => {
    memberQueryMock.mockResolvedValueOnce([]);

    const procedure = authorizedProcedure.handler(({ context }) => ({
      canReadTodo: context.ability.can("read", "Todo"),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-1" }),
        ...createBaseContext(),
      },
    });
    expect(result.canReadTodo).toBe(false);
  });

  it("filters out unknown RBAC role strings", async () => {
    const procedure = authorizedProcedure.handler(({ context }) => ({
      canManageUser: context.ability.can("manage", "User"),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: "superadmin" }),
        ...createBaseContext(),
      },
    });
    expect(result.canManageUser).toBe(false);
  });

  it("keeps valid roles when mixed with invalid ones", async () => {
    const procedure = authorizedProcedure.handler(({ context }) => ({
      canManageUser: context.ability.can("manage", "User"),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: "admin,bogus" }),
        ...createBaseContext(),
      },
    });
    expect(result.canManageUser).toBe(true);
  });

  it("treats an empty role string as no roles", async () => {
    const procedure = authorizedProcedure.handler(({ context }) => ({
      canManageUser: context.ability.can("manage", "User"),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ role: "" }),
        ...createBaseContext(),
      },
    });
    expect(result.canManageUser).toBe(false);
  });

  it("grants org-admin Todo update but not delete when member.role is admin", async () => {
    memberQueryMock.mockResolvedValueOnce([{ role: "admin" }]);

    const procedure = authorizedProcedure.handler(({ context }) => ({
      canUpdateTodo: context.ability.can(
        "update",
        subject("Todo", {
          id: "t-1",
          text: "t",
          status: "todo",
          label: "documentation",
          priority: "medium",
          estimated_hours: 0,
          actual_hours: 0,
          progress: 0,
          cost: 0,
          due_date: null,
          completed_at: null,
          created_at: new Date(),
          updated_at: new Date(),
          organization_id: "org-1",
        }),
      ),
      canDeleteTodo: context.ability.can(
        "delete",
        subject("Todo", {
          id: "t-1",
          text: "t",
          status: "todo",
          label: "documentation",
          priority: "medium",
          estimated_hours: 0,
          actual_hours: 0,
          progress: 0,
          cost: 0,
          due_date: null,
          completed_at: null,
          created_at: new Date(),
          updated_at: new Date(),
          organization_id: "org-1",
        }),
      ),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-1" }),
        ...createBaseContext(),
      },
    });
    expect(result.canUpdateTodo).toBe(true);
    expect(result.canDeleteTodo).toBe(false);
  });

  it("grants org-member Todo read but not update when member.role is member", async () => {
    memberQueryMock.mockResolvedValueOnce([{ role: "member" }]);

    const procedure = authorizedProcedure.handler(({ context }) => ({
      canReadTodo: context.ability.can(
        "read",
        subject("Todo", {
          id: "t-1",
          text: "t",
          status: "todo",
          label: "documentation",
          priority: "medium",
          estimated_hours: 0,
          actual_hours: 0,
          progress: 0,
          cost: 0,
          due_date: null,
          completed_at: null,
          created_at: new Date(),
          updated_at: new Date(),
          organization_id: "org-1",
        }),
      ),
      canUpdateTodo: context.ability.can(
        "update",
        subject("Todo", {
          id: "t-1",
          text: "t",
          status: "todo",
          label: "documentation",
          priority: "medium",
          estimated_hours: 0,
          actual_hours: 0,
          progress: 0,
          cost: 0,
          due_date: null,
          completed_at: null,
          created_at: new Date(),
          updated_at: new Date(),
          organization_id: "org-1",
        }),
      ),
    }));

    const result = await call(procedure, undefined, {
      context: {
        session: createSession({ activeOrganizationId: "org-1" }),
        ...createBaseContext(),
      },
    });
    expect(result.canReadTodo).toBe(true);
    expect(result.canUpdateTodo).toBe(false);
  });

  it("propagates DB rejection without wrapping it", async () => {
    const dbError = new Error("connection refused");
    memberQueryMock.mockRejectedValueOnce(dbError);

    const procedure = authorizedProcedure.handler(() => ({ ok: true }));

    await Promise.resolve(
      expect(
        call(procedure, undefined, {
          context: {
            session: createSession({ activeOrganizationId: "org-1" }),
            ...createBaseContext(),
          },
        }),
      ).rejects.toThrow("connection refused"),
    );
  });
});
