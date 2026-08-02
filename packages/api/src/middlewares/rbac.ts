import { o } from "@lindaflor/api/middlewares";
import { auth } from "@lindaflor/auth";
import type { RBACPermissionInputsAtLeastOne } from "@lindaflor/shared/lib/permissions";
import { isRole, type Roles } from "@lindaflor/shared/lib/roles";
import { ORPCError } from "@orpc/server";

export const rbacMiddleware = (permissions: RBACPermissionInputsAtLeastOne) =>
  o.middleware(async ({ context, next }) => {
    if (!context.session) {
      throw new ORPCError("UNAUTHORIZED", {
        message: "Você precisa estar autenticado",
      });
    }

    const roles = (context.session.user.role ?? "user")
      .split(",")
      .map((r) => r.trim())
      .filter((r): r is Roles => r !== "" && isRole(r));

    const userId = context.session.user.id;

    // Users may have multiple roles, and the order of these roles can
    // affect permission validation. Check all roles regardless of order to
    // avoid inconsistent validation results.
    const hasPermission = await Promise.all(
      roles.map((role) =>
        auth.api.userHasPermission({
          body: { userId, role, permissions },
        }),
      ),
    );

    if (!hasPermission.some((p) => p.success)) {
      throw new ORPCError("FORBIDDEN", {
        message: "Você não tem permissão para acessar este recurso",
      });
    }

    return next({ context });
  });
