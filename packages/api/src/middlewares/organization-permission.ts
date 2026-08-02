import { o } from "@lindaflor/api/middlewares";
import { auth } from "@lindaflor/auth";
import type { OrganizationPermissionInputsAtLeastOne } from "@lindaflor/shared/lib/permissions";
import { ORPCError } from "@orpc/server";

export const organizationMiddleware = (
  permissions: OrganizationPermissionInputsAtLeastOne,
) =>
  o.middleware(async ({ context, next }) => {
    const session = context.session;
    const activeOrganizationId = session?.session.activeOrganizationId;
    if (!session || !activeOrganizationId) {
      throw new ORPCError("UNAUTHORIZED", {
        message: "Você precisa estar em uma organização",
      });
    }

    const hasPermission = await auth.api.hasPermission({
      headers: context.headers,
      body: {
        permissions: { ...permissions },
        organizationId: activeOrganizationId,
      },
    });

    if (!hasPermission) {
      throw new ORPCError("FORBIDDEN", {
        message: "Você não tem permissão para acessar este recurso",
      });
    }

    return next({
      context: {
        ...context,
        session: {
          ...session,
          session: {
            ...session.session,
            activeOrganizationId,
          },
        },
      },
    });
  });
