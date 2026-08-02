import { tank_day_bulletin_statuses } from "@lindaflor/shared/enums/tankage";
import { z } from "zod";

const rawRowSchema = z.object({
  id: z.guid(),
  tank_id: z.guid(),
  organization_id: z.guid(),
  operational_day: z.iso.date(),
  transferred_at: z.date(),
  height_before_m: z.number(),
  height_after_m: z.number(),
  oil_temperature_c: z.number(),
  ambient_temperature_c: z.number(),
  gross_volume_before_m3: z.number(),
  gross_volume_after_m3: z.number(),
  gross_volume_out_m3: z.number(),
  gross_volume_out_m3_20c: z.number().nullable(),
  net_oil_volume_out_m3_20c: z.number().nullable(),
  shell_temperature_c: z.number().nullable(),
  shell_correction_factor: z.number().nullable(),
  liquid_correction_factor: z.number().nullable(),
  combined_correction_factor: z.number().nullable(),
  tank_calibration_id: z.guid().nullable(),
  lab_oil_analysis_id: z.guid().nullable(),
  density_at_20c_kg_m3: z.number().nullable(),
  water_and_sediment_percent: z.number().nullable(),
  destination_label: z.string().nullable(),
  observation: z.string(),
  tankage_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});

const rowSchema = rawRowSchema.extend({
  bulletin_status: z.enum(tank_day_bulletin_statuses),
});

export type TankTransferOutput = z.infer<typeof rowSchema>;

export const schema = {
  listBy: {
    tank: {
      input: z.object({
        tank_id: z.guid(),
        operational_day: z.iso.date().optional(),
      }),
      output: z.object({ data: z.array(rowSchema) }),
    },
  },
  create: {
    input: z.object({
      tank_id: z.guid(),
      transferred_at: z.date(),
      height_before_m: z.number().min(0),
      height_after_m: z.number().min(0),
      oil_temperature_c: z.number(),
      ambient_temperature_c: z.number(),
      destination_label: z.string().trim().nullable().optional(),
      observation: z
        .string()
        .min(1, { message: "Informe a observação" })
        .trim(),
      operator_user_id: z.guid(),
      measurement_equipment_id: z.guid().nullable().optional(),
    }),
    output: rowSchema,
  },
  retreat: {
    input: z
      .object({
        id: z.guid(),
        transferred_at: z.date().optional(),
        height_before_m: z.number().min(0).optional(),
        height_after_m: z.number().min(0).optional(),
        oil_temperature_c: z.number().optional(),
        ambient_temperature_c: z.number().optional(),
        destination_label: z.string().trim().nullable().optional(),
        observation: z.string().min(1).trim().optional(),
        justification: z
          .string()
          .min(1, { message: "Informe a justificativa do retratamento" })
          .trim(),
      })
      .refine(
        (value) =>
          value.transferred_at != null ||
          value.height_before_m != null ||
          value.height_after_m != null ||
          value.oil_temperature_c != null ||
          value.ambient_temperature_c != null ||
          value.destination_label !== undefined ||
          value.observation != null,
        { message: "Informe ao menos um campo para corrigir" },
      ),
    output: rowSchema,
  },
  delete: {
    input: z.object({ id: z.guid() }),
    output: z.null(),
  },
};
