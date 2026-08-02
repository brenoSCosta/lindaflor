import { protectedProcedure } from "@lindaflor/api/middlewares/auth";

export const privateData = protectedProcedure.handler(({ context }) => {
  return {
    message: "Isso é privado",
    user: context.session?.user,
  };
});
