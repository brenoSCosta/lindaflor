import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import { applyUploadOperation } from "@lindaflor/api/powersync/apply-upload-operation";
import { getPowerSyncCredentials } from "@lindaflor/core/powersync/credentials";
import { schema, type UploadError } from "@lindaflor/shared/schemas/powersync";
import type { EnhancedRouter } from "@orpc/server";

const powersyncV1Routes = {
  credentials: authorizedProcedure
    .use(requireActiveOrganization())
    .route({
      method: "GET",
      path: "/powersync/credentials",
      description: "PowerSync JWT credentials for the authenticated user",
      summary: "v1 Credentials",
    })
    .output(schema.v1.credentials.output)
    .handler(async ({ context }) =>
      getPowerSyncCredentials({ headers: context.headers }),
    ),

  upload: authorizedProcedure
    .use(requireActiveOrganization())
    .route({
      method: "POST",
      path: "/powersync/upload",
      description: "Apply offline PowerSync write operations to Postgres",
      summary: "v1 Upload",
    })
    .input(schema.v1.upload.input)
    .output(schema.v1.upload.output)
    .handler(async ({ input, context }) => {
      const errors: UploadError[] = [];
      // Sequential on purpose: preserve PowerSync CRUD order within a batch.
      for (const operation of input.operations) {
        // oxlint-disable-next-line react-doctor/async-await-in-loop, eslint/no-await-in-loop -- ordered batch apply
        const error = await applyUploadOperation(operation, {
          activeOrganizationId: context.activeOrganizationId,
          createdByUserId: context.session.user.id,
          ability: context.ability,
          clientTimezone: context.client.timezone,
        });
        if (error != null) {
          errors.push(error);
        }
      }

      return schema.v1.upload.output.parse({
        success: errors.length === 0,
        errors: errors.length > 0 ? errors : undefined,
      });
    }),
};

type PowerSyncV1Routes = typeof powersyncV1Routes;

type PowerSyncV1Router = EnhancedRouter<
  PowerSyncV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type PowerSyncRouter = {
  v1: PowerSyncV1Router;
};

function createPowerSyncV1Router(routes: PowerSyncV1Routes): PowerSyncV1Router {
  return o.prefix("/v1").tag("PowerSync").router(routes);
}

export const powersyncRouter: PowerSyncRouter = {
  v1: createPowerSyncV1Router(powersyncV1Routes),
};
