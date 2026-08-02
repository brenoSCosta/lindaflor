import { o } from "@lindaflor/api/middlewares";
import { protectedProcedure } from "@lindaflor/api/middlewares/auth";
import { ORPCError } from "@orpc/server";

const requireOrganization = o.middleware(async ({ context, next }) => {
  if (!context.session?.session.activeOrganizationId) {
    throw new ORPCError("UNAUTHORIZED", {
      message: "Você precisa estar em uma organização",
    });
  }
  return next({
    context: {
      ...context,
      session: {
        ...context.session,
        session: {
          ...context.session.session,
          activeOrganizationId: context.session.session.activeOrganizationId,
        },
      },
    },
  });
});

export const organizationProcedure = protectedProcedure
  .use(requireOrganization)
  .errors({
    UNAUTHORIZED: {
      message: "Você precisa estar em uma organização",
    },
  });
