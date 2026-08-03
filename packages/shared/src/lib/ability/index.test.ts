import { describe, expect, it } from "bun:test";

import { defineAbilityFor } from "@lindaflor/shared/lib/ability";
import type {
  AbilityMember,
  AbilityOrganization,
  AbilityUser,
} from "@lindaflor/shared/lib/ability/subjects";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import type { TodoOutput } from "@lindaflor/shared/schemas/todo";

const todoInOrg = (organization_id: string, id = "todo-1"): TodoOutput => ({
  id,
  text: "test todo",
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
  organization_id,
});

const memberInOrg = (
  organization_id: string,
  id = "member-1",
): AbilityMember => ({
  id,
  organization_id,
  user_id: "user-x",
  role: "member",
  created_at: new Date(),
});

const userRow = (id = "other-user"): AbilityUser => ({
  id,
  name: "Test User",
  email: `${id}@example.com`,
  email_verified: true,
  image: null,
  created_at: new Date(),
  updated_at: new Date(),
  role: "user",
  banned: false,
  ban_reason: null,
  ban_expires: null,
  two_factor_enabled: false,
});

const orgRow = (id = "org-1"): AbilityOrganization => ({
  id,
  name: "Test Organization",
  slug: `test-org-${id}`,
  logo: null,
  created_at: new Date(),
  metadata: null,
});

describe("defineAbilityFor", () => {
  describe("admin RBAC role", () => {
    it("with no active org, cannot touch org-scoped Todos", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-x")))).toBe(
        false,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-x")))).toBe(
        false,
      );
    });

    it("who is owner of active org can manage that org Todos and still manage Users", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("read", subject("Todo", todoInOrg("org-2")))).toBe(
        false,
      );
      expect(ability.can("manage", "User")).toBe(true);
    });

    it("who is member of active org gets member-level Todo access", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
      expect(ability.can("manage", "User")).toBe(true);
    });
  });

  describe("org owner", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "owner" as const,
    };

    it("can manage Todo in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "Todo")).toBe(true);
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("update", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
    });

    it("cannot touch Todo from a different org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("read", subject("Todo", todoInOrg("org-2")))).toBe(
        false,
      );
      expect(ability.can("delete", subject("Todo", todoInOrg("org-2")))).toBe(
        false,
      );
    });
  });

  describe("org admin", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "admin" as const,
    };

    it("can create, read, update Todo in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "Todo")).toBe(true);
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("update", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
    });

    it("cannot delete Todo (owner-only)", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("delete", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
    });
  });

  describe("org member", () => {
    const actor = {
      userId: "u",
      roles: ["user"] as const,
      activeOrganizationId: "org-1",
      orgRole: "member" as const,
    };

    it("can create and read Todo in active org", () => {
      const ability = defineAbilityFor(actor);
      expect(ability.can("create", "Todo")).toBe(true);
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        true,
      );
    });
  });

  describe("no active org", () => {
    it("cannot do anything on Todo", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", "Todo")).toBe(false);
      expect(ability.can("create", "Todo")).toBe(false);
    });
  });

  describe("active org but not a member of it", () => {
    it("cannot do anything on Todo", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: null,
      });
      expect(ability.can("read", subject("Todo", todoInOrg("org-1")))).toBe(
        false,
      );
    });
  });

  describe("Member subject", () => {
    it("admin RBAC cannot manage members in any org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(
        ability.can("update", subject("Member", memberInOrg("org-x"))),
      ).toBe(false);
      expect(
        ability.can("delete", subject("Member", memberInOrg("org-y"))),
      ).toBe(false);
      expect(ability.can("read", subject("Member", memberInOrg("org-x")))).toBe(
        false,
      );
    });

    it("org owner can invite, update, and remove members in active org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(ability.can("create", "Member")).toBe(true);
      expect(
        ability.can("update", subject("Member", memberInOrg("org-1"))),
      ).toBe(true);
      expect(
        ability.can("delete", subject("Member", memberInOrg("org-1"))),
      ).toBe(true);
    });

    it("org owner cannot touch members in a different org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(
        ability.can("update", subject("Member", memberInOrg("org-2"))),
      ).toBe(false);
    });

    it("org admin can create (invite) and read but not update or delete members", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "admin",
      });
      // The invite button gates on the type-level check.
      expect(ability.can("create", "Member")).toBe(true);
      expect(
        ability.can("create", subject("Member", memberInOrg("org-1"))),
      ).toBe(true);
      expect(
        ability.can("create", subject("Member", memberInOrg("org-2"))),
      ).toBe(false);
      expect(ability.can("read", subject("Member", memberInOrg("org-1")))).toBe(
        true,
      );
      expect(
        ability.can("update", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
      expect(
        ability.can("delete", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
    });

    it("org member can only read members (cannot invite)", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(ability.can("read", subject("Member", memberInOrg("org-1")))).toBe(
        true,
      );
      expect(ability.can("create", "Member")).toBe(false);
      expect(
        ability.can("update", subject("Member", memberInOrg("org-1"))),
      ).toBe(false);
    });
  });

  describe("moderator RBAC role", () => {
    it("cannot manage Users", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["moderator"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", subject("User", userRow()))).toBe(false);
      expect(ability.can("manage", "User")).toBe(false);
    });
  });

  describe("User subject", () => {
    const adminAbility = defineAbilityFor({
      userId: "admin-1",
      roles: ["admin"],
      activeOrganizationId: null,
      orgRole: null,
    });

    it("admin can update, ban, delete, impersonate other users", () => {
      const other = userRow("other-1");
      expect(adminAbility.can("update", subject("User", other))).toBe(true);
      expect(adminAbility.can("ban", subject("User", other))).toBe(true);
      expect(adminAbility.can("delete", subject("User", other))).toBe(true);
      expect(adminAbility.can("impersonate", subject("User", other))).toBe(
        true,
      );
    });

    it("admin can read and update themselves", () => {
      const self = userRow("admin-1");
      expect(adminAbility.can("read", subject("User", self))).toBe(true);
      expect(adminAbility.can("update", subject("User", self))).toBe(true);
    });

    it("admin cannot delete, ban, or impersonate themselves", () => {
      const self = userRow("admin-1");
      expect(adminAbility.can("delete", subject("User", self))).toBe(false);
      expect(adminAbility.can("ban", subject("User", self))).toBe(false);
      expect(adminAbility.can("impersonate", subject("User", self))).toBe(
        false,
      );
    });

    it("non-admin RBAC user has no User access", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(ability.can("read", subject("User", userRow("any")))).toBe(false);
      expect(ability.can("update", subject("User", userRow("any")))).toBe(
        false,
      );
      expect(ability.can("ban", subject("User", userRow("any")))).toBe(false);
      expect(ability.can("delete", subject("User", userRow("any")))).toBe(
        false,
      );
      expect(ability.can("impersonate", subject("User", userRow("any")))).toBe(
        false,
      );
    });
  });

  describe("Organization subject", () => {
    it("org owner can read and update Organization in active org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
    });

    it("org owner cannot update Organization in a different org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "owner",
      });
      expect(
        ability.can("update", subject("Organization", orgRow("org-2"))),
      ).toBe(false);
    });

    it("org admin can read and update Organization in active org", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "admin",
      });
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
    });

    it("org member can only read, not update Organization", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: "org-1",
        orgRole: "member",
      });
      expect(
        ability.can("read", subject("Organization", orgRow("org-1"))),
      ).toBe(true);
      expect(
        ability.can("update", subject("Organization", orgRow("org-1"))),
      ).toBe(false);
    });

    it("no active org cannot access Organization", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["user"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", "Organization")).toBe(false);
      expect(ability.can("update", "Organization")).toBe(false);
    });

    it("global admin cannot access Organization without active org role", () => {
      const ability = defineAbilityFor({
        userId: "u",
        roles: ["admin"],
        activeOrganizationId: null,
        orgRole: null,
      });
      expect(ability.can("read", "Organization")).toBe(false);
      expect(ability.can("update", "Organization")).toBe(false);
    });
  });
});
