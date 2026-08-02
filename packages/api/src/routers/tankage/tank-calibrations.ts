import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createCalibration,
  deleteCalibrations,
  getCalibrationById,
  listCalibrationsByTank,
  listCurrentCalibrations,
  replaceCalibrationPoints,
  resolveCalibrationVolume,
  updateCalibration,
} from "@lindaflor/core/tankage/tank-calibrations";
import { schema } from "@lindaflor/shared/schemas/tankage";

export const calibrationRouter = {
  list: {
    current: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "TankCalibrations"))
      .route({
        method: "GET",
        path: "/tank-calibrations/current",
        description:
          "List current (valid on date) calibration summaries for the org",
        summary: "v1 ListCurrentSummaries",
      })
      .input(schema.v1.calibration.list.current.input)
      .output(schema.v1.calibration.list.current.output)
      .handler(async ({ input, context }) =>
        listCurrentCalibrations({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },

  listBy: {
    tank: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "TankCalibrations"))
      .route({
        method: "GET",
        path: "/tank-calibrations",
        description: "List tank calibration certificates by tank",
        summary: "v1 ListByTank",
      })
      .input(schema.v1.calibration.listBy.tank.input)
      .output(schema.v1.calibration.listBy.tank.output)
      .handler(async ({ input, context }) =>
        listCalibrationsByTank({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },

  getBy: {
    id: authorizedProcedure
      .route({
        method: "GET",
        path: "/tank-calibrations/{id}",
        description: "Get tank calibration with points",
        summary: "v1 GetById",
      })
      .input(schema.v1.calibration.getBy.id.input)
      .output(schema.v1.calibration.getBy.id.output)
      .handler(async ({ input, context }) =>
        getCalibrationById({ input, ability: context.ability }),
      ),
  },

  create: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("create", "TankCalibrations"))
    .route({
      method: "POST",
      path: "/tank-calibrations",
      description: "Create a tank calibration certificate",
      summary: "v1 Create",
    })
    .input(schema.v1.calibration.create.input)
    .output(schema.v1.calibration.create.output)
    .handler(async ({ input, context }) =>
      createCalibration({
        input,
        organizationId: context.activeOrganizationId,
        createdByUserId: context.session.user.id,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/tank-calibrations",
      description: "Update tank calibration metadata",
      summary: "v1 Update",
    })
    .input(schema.v1.calibration.update.input)
    .output(schema.v1.calibration.update.output)
    .handler(async ({ input, context }) =>
      updateCalibration({ input, ability: context.ability }),
    ),

  replace: {
    point: authorizedProcedure
      .route({
        method: "PUT",
        path: "/tank-calibrations/points",
        description: "Replace calibration height/volume points",
        summary: "v1 ReplacePoints",
      })
      .input(schema.v1.calibration.replace.point.input)
      .output(schema.v1.calibration.replace.point.output)
      .handler(async ({ input, context }) =>
        replaceCalibrationPoints({ input, ability: context.ability }),
      ),
  },

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/tank-calibrations",
      description: "Delete tank calibrations",
      summary: "v1 Delete",
    })
    .input(schema.v1.calibration.delete.input)
    .output(schema.v1.calibration.delete.output)
    .handler(async ({ input, context }) =>
      deleteCalibrations({ input, ability: context.ability }),
    ),

  resolve: {
    volume: authorizedProcedure
      .use(requireActiveOrganization())
      .use(authorize("read", "TankCalibrations"))
      .route({
        method: "POST",
        path: "/tank-calibrations/resolve-volume",
        description:
          "Resolve gross volume from height (m) via arqueação table (cm)",
        summary: "v1 ResolveVolume",
      })
      .input(schema.v1.calibration.resolve.volume.input)
      .output(schema.v1.calibration.resolve.volume.output)
      .handler(async ({ input, context }) =>
        resolveCalibrationVolume({
          input,
          organizationId: context.activeOrganizationId,
        }),
      ),
  },
};
