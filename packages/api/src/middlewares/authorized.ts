import type { Context } from "@lindaflor/api/context";
import { protectedProcedure } from "@lindaflor/api/middlewares/auth";
import { db } from "@lindaflor/db";
import { members } from "@lindaflor/db/schema/auth";
import { defineAbilityFor } from "@lindaflor/shared/lib/ability";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import {
  parseRoles,
  toOrgRole,
  type OrgRoles,
} from "@lindaflor/shared/lib/roles";
import { os } from "@orpc/server";
import { and, eq } from "drizzle-orm";

export type AuthorizedContext = Omit<Context, "session"> & {
  session: NonNullable<Context["session"]>;
  ability: AppAbility;
};

export const oAuthorized = os.$context<AuthorizedContext>();

export const authorizedProcedure = protectedProcedure
  .use(async ({ context, next }) => {
    const session = context.session;

    const roles = parseRoles(session.user.role);

    const activeOrganizationId = session.session.activeOrganizationId ?? null;

    let orgRole: OrgRoles | null = null;
    if (activeOrganizationId) {
      const [row] = await db
        .select({ role: members.role })
        .from(members)
        .where(
          and(
            eq(members.user_id, session.user.id),
            eq(members.organization_id, activeOrganizationId),
          ),
        )
        .limit(1);
      orgRole = toOrgRole(row?.role);
    }

    const ability = defineAbilityFor({
      userId: session.user.id,
      roles,
      activeOrganizationId,
      orgRole,
    });

    return next({ context: { ...context, ability } });
  })
  .errors({
    FORBIDDEN: { message: "Você não tem permissão para executar esta ação" },
  });
