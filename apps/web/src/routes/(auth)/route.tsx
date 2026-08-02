import { defineAbilityFor } from "@lindaflor/shared/lib/ability";
import { parseRoles, toOrgRole } from "@lindaflor/shared/lib/roles";
import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/(auth)")({
  beforeLoad: async ({ context }) => {
    const result = await context.auth.getSession();
    if (result.error || !result.data) {
      throw redirect({ to: "/login" });
    }
    const memberResult = await context.auth.organization.getActiveMember();
    const ability = defineAbilityFor({
      userId: result.data.user.id,
      roles: parseRoles(result.data.user.role),
      activeOrganizationId: result.data.session.activeOrganizationId ?? null,
      orgRole: toOrgRole(memberResult.data?.role),
    });
    return { session: result.data, ability };
  },
});
