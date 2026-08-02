import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createMeasurementEquipment,
  deleteMeasurementEquipments,
  getAllMeasurementEquipments,
  getMeasurementEquipmentById,
  updateMeasurementEquipment,
} from "@lindaflor/core/measurement-equipment/measurement-equipments";
import { schema } from "@lindaflor/shared/schemas/measurement-equipment";
import type { EnhancedRouter } from "@orpc/server";

const measurementEquipmentsV1Routes = {
  getAll: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("read", "MeasurementEquipments"))
    .route({
      method: "GET",
      path: "/measurement-equipments",
      description: "Get all measurement equipments (trenas)",
      summary: "v1 GetAll",
    })
    .input(schema.v1.getAll.input)
    .output(schema.v1.getAll.output)
    .handler(async ({ input, context }) =>
      getAllMeasurementEquipments({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  getById: authorizedProcedure
    .route({
      method: "GET",
      path: "/measurement-equipments/{id}",
      description: "Get measurement equipment by ID",
      summary: "v1 GetById",
    })
    .input(schema.v1.getById.input)
    .output(schema.v1.getById.output)
    .handler(async ({ input, context }) =>
      getMeasurementEquipmentById({ input, ability: context.ability }),
    ),

  create: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("create", "MeasurementEquipments"))
    .route({
      method: "POST",
      path: "/measurement-equipments",
      description: "Create a measurement equipment (trena)",
      summary: "v1 Create",
    })
    .input(schema.v1.create.input)
    .output(schema.v1.create.output)
    .handler(async ({ input, context }) =>
      createMeasurementEquipment({
        input,
        organizationId: context.activeOrganizationId,
        userId: context.session.user.id,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/measurement-equipments",
      description: "Update a measurement equipment",
      summary: "v1 Update",
    })
    .input(schema.v1.update.input)
    .output(schema.v1.update.output)
    .handler(async ({ input, context }) =>
      updateMeasurementEquipment({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/measurement-equipments",
      description: "Delete measurement equipments",
      summary: "v1 Delete",
    })
    .input(schema.v1.delete.input)
    .output(schema.v1.delete.output)
    .handler(async ({ input, context }) =>
      deleteMeasurementEquipments({ input, ability: context.ability }),
    ),
};

type MeasurementEquipmentsV1Routes = typeof measurementEquipmentsV1Routes;

type MeasurementEquipmentsV1Router = EnhancedRouter<
  MeasurementEquipmentsV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type MeasurementEquipmentsRouter = {
  v1: MeasurementEquipmentsV1Router;
};

function createMeasurementEquipmentsV1Router(
  routes: MeasurementEquipmentsV1Routes,
): MeasurementEquipmentsV1Router {
  return o.prefix("/v1").tag("MeasurementEquipments").router(routes);
}

export const measurementEquipmentsRouter: MeasurementEquipmentsRouter = {
  v1: createMeasurementEquipmentsV1Router(measurementEquipmentsV1Routes),
};
