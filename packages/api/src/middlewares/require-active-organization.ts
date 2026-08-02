import { oAuthorized } from "@lindaflor/api/middlewares/authorized";
import { ORPCError } from "@orpc/server";

export const requireActiveOrganization = () =>
  oAuthorized.middleware(async ({ context, next }) => {
    const activeOrganizationId = context.session.session.activeOrganizationId;
    if (!activeOrganizationId) {
      throw new ORPCError("FORBIDDEN", {
        message: "Você precisa estar em uma organização",
      });
    }
    return next({ context: { ...context, activeOrganizationId } });
  });
