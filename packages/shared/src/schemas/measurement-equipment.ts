import { measurement_equipment_types } from "@lindaflor/shared/enums/tankage";
import { globalFilter, pagination } from "@lindaflor/shared/lib/utils";
import { z } from "zod";

const positiveMeters = z
  .number()
  .positive({ message: "Informe um valor maior que zero" });

const equipmentTypeSchema = z.enum(measurement_equipment_types);

export const rowSchema = z.object({
  id: z.guid(),
  code: z.string(),
  description: z.string().nullable(),
  type: equipmentTypeSchema,
  length_m: z.number().nullable(),
  reference_height_m: z.number().nullable(),
  manufacturer: z.string().nullable(),
  serial_number: z.string().nullable(),
  calibrated_at: z.iso.date().nullable(),
  calibration_valid_until: z.iso.date().nullable(),
  active: z.boolean(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type MeasurementEquipmentOutput = z.infer<typeof rowSchema>;

const equipmentFields = {
  type: equipmentTypeSchema.optional(),
  length_m: positiveMeters.nullable().optional(),
  reference_height_m: z.number().nullable().optional(),
  manufacturer: z.string().trim().nullable().optional(),
  serial_number: z.string().trim().nullable().optional(),
  calibrated_at: z.iso.date().nullable().optional(),
  calibration_valid_until: z.iso.date().nullable().optional(),
  active: z.boolean().optional(),
};

export const schema = {
  v1: {
    getAll: {
      input: z
        .object({
          pagination,
          globalFilter,
        })
        .optional(),
      output: z.object({
        data: z.array(rowSchema),
      }),
    },

    getById: {
      input: z.object({
        id: z.guid(),
      }),
      output: rowSchema,
    },

    create: {
      input: z
        .object({
          code: z
            .string()
            .min(1, { message: "Informe o código da trena" })
            .trim(),
          description: z.string().trim().optional(),
          ...equipmentFields,
        })
        .refine(
          (value) =>
            value.calibrated_at == null ||
            value.calibration_valid_until == null ||
            value.calibration_valid_until >= value.calibrated_at,
          {
            message:
              "Validade da calibração deve ser igual ou posterior à data de calibração",
            path: ["calibration_valid_until"],
          },
        ),
      output: rowSchema,
    },

    update: {
      input: z
        .object({
          id: z.guid(),
          code: z
            .string()
            .min(1, { message: "Informe o código da trena" })
            .trim()
            .optional(),
          description: z.string().trim().nullable().optional(),
          ...equipmentFields,
        })
        .refine(
          (value) =>
            value.calibrated_at == null ||
            value.calibration_valid_until == null ||
            value.calibration_valid_until >= value.calibrated_at,
          {
            message:
              "Validade da calibração deve ser igual ou posterior à data de calibração",
            path: ["calibration_valid_until"],
          },
        ),
      output: rowSchema,
    },

    delete: {
      input: z.object({
        ids: z.array(z.guid()).min(1),
      }),
      output: z.null(),
    },
  },
};

export const MEASUREMENT_EQUIPMENT_TYPE_LABELS: Record<
  (typeof measurement_equipment_types)[number],
  string
> = {
  manual: "Manual",
  electronic: "Eletrônica",
};
