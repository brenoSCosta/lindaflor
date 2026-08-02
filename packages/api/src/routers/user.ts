import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { protectedProcedure } from "@lindaflor/api/middlewares/auth";
import { getAvatarUrl, updateAvatar } from "@lindaflor/core/user/avatar";
import { schema } from "@lindaflor/shared/schemas/user";
import type { EnhancedRouter } from "@orpc/server";

const userV1Routes = {
  avatar: o.router({
    update: protectedProcedure
      .route({
        method: "POST",
        path: "/users/me/avatar",
        description: "Update current user avatar",
        summary: "v1 UpdateAvatar",
      })
      .input(schema.v1.avatar.update.input)
      .output(schema.v1.avatar.update.output)
      .handler(async ({ input, context }) =>
        updateAvatar({
          input,
          userId: context.session.user.id,
        }),
      ),

    get: protectedProcedure
      .route({
        method: "GET",
        path: "/users/{userId}/avatar-url",
        description: "Get avatar URL for a user",
        summary: "v1 GetAvatarUrl",
      })
      .input(schema.v1.avatar.get.input)
      .output(schema.v1.avatar.get.output)
      .handler(async ({ input, context }) =>
        getAvatarUrl({
          input,
          sessionUserId: context.session.user.id,
          isAdmin: context.session.user.role?.includes("admin") ?? false,
        }),
      ),
  }),
};

type UserV1Routes = typeof userV1Routes;

type UserV1Router = EnhancedRouter<
  UserV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type UserRouter = {
  v1: UserV1Router;
};

function createUserV1Router(routes: UserV1Routes): UserV1Router {
  return o.prefix("/v1").tag("User").router(routes);
}

export const userRouter: UserRouter = {
  v1: createUserV1Router(userV1Routes),
};
