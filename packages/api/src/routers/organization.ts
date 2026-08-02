import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { protectedProcedure } from "@lindaflor/api/middlewares/auth";
import {
  getOrganizationLogoUrl,
  updateOrganizationLogo,
} from "@lindaflor/core/organization/logo";
import { schema } from "@lindaflor/shared/schemas/organization";
import type { EnhancedRouter } from "@orpc/server";

const organizationV1Routes = {
  logo: o.router({
    update: protectedProcedure
      .route({
        method: "POST",
        path: "/organizations/logo",
        description: "Update organization logo",
        summary: "v1 UpdateOrganizationLogo",
      })
      .input(schema.v1.logo.update.input)
      .output(schema.v1.logo.update.output)
      .handler(async ({ input, context }) =>
        updateOrganizationLogo({
          input,
          userId: context.session.user.id,
        }),
      ),

    get: protectedProcedure
      .route({
        method: "GET",
        path: "/organizations/{id}/logo-url",
        description: "Get logo URL for an organization",
        summary: "v1 GetOrganizationLogoUrl",
      })
      .input(schema.v1.logo.get.input)
      .output(schema.v1.logo.get.output)
      .handler(async ({ input, context }) =>
        getOrganizationLogoUrl({
          input,
          userId: context.session.user.id,
        }),
      ),
  }),
};

type OrganizationV1Routes = typeof organizationV1Routes;

type OrganizationV1Router = EnhancedRouter<
  OrganizationV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type OrganizationRouter = {
  v1: OrganizationV1Router;
};

function createOrganizationV1Router(
  routes: OrganizationV1Routes,
): OrganizationV1Router {
  return o.prefix("/v1").tag("Organization").router(routes);
}

export const organizationRouter: OrganizationRouter = {
  v1: createOrganizationV1Router(organizationV1Routes),
};
