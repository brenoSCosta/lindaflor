import { o, publicProcedure } from "@lindaflor/api/middlewares";
import { ORPCError } from "@orpc/server";

const requireAuth = o.middleware(async ({ context, next }) => {
  if (!context.session) {
    throw new ORPCError("UNAUTHORIZED", {
      message: "Você precisa estar autenticado",
    });
  }
  if (context.session.user.banned) {
    throw new ORPCError("FORBIDDEN", {
      message: "Sua conta foi banida",
    });
  }
  return next({
    context: {
      ...context,
      session: context.session,
    },
  });
});

export const protectedProcedure = publicProcedure.use(requireAuth).errors({
  UNAUTHORIZED: { message: "Você precisa estar autenticado" },
  FORBIDDEN: { message: "Sua conta foi banida" },
});
