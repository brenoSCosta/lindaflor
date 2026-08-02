import {
  columnFilters,
  facetsSchema,
  globalFilter,
  keepPinnedRows,
  pagination,
  rowPinning,
  sorting,
} from "@lindaflor/shared/lib/utils";
import { z } from "zod";

const latitudeSchema = z.number().min(-90).max(90);
const longitudeSchema = z.number().min(-180).max(180);

export const rawRowSchema = z.object({
  id: z.guid(),
  tag: z.string(),
  concession_id: z.guid(),
  installation_id: z.guid(),
  measurement_equipment_id: z.guid().nullable(),
  latitude: z.number().nullable(),
  longitude: z.number().nullable(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});

export const rowSchema = rawRowSchema.extend({
  concession_name: z.string(),
  installation_name: z.string(),
  measurement_equipment_code: z.string().nullable(),
});
export type TankOutput = z.infer<typeof rowSchema>;

const calibrationStatusSchema = z.enum([
  "current",
  "expired",
  "future",
  "none",
]);

const snapshotSchema = rowSchema.extend({
  capacity_volume_m3: z.number().nullable(),
  capacity_height_m: z
    .number()
    .nullable()
    .default(null)
    .describe(
      "Maior altura (m) da tabela de arqueação vigente: max(height_cm) / 100",
    ),
  current_height_m: z.number().nullable(),
  current_volume_m3: z.number().nullable(),
  current_net_oil_volume_m3_20c: z.number().nullable(),
  current_volume_oil_barrels: z.number().nullable(),
  today_production_gross_volume_m3: z.number().nullable(),
  today_production_net_oil_volume_m3_20c: z.number().nullable(),
  today_production_volume_oil_barrels: z.number().nullable(),
  calibration_status: calibrationStatusSchema,
  calibration_certificate_number: z.string().nullable(),
  calibration_valid_until: z.iso.date().nullable(),
});
export type TankSnapshotOutput = z.infer<typeof snapshotSchema>;

export const schema = {
  list: {
    all: {
      input: z
        .object({
          pagination,
          sorting,
          columnFilters,
          globalFilter,
          rowPinning,
          keepPinnedRows,
        })
        .optional(),
      output: z.object({
        data: z.array(rowSchema),
        meta: z
          .object({
            rowCount: z.number(),
            facets: facetsSchema,
          })
          .optional(),
      }),
    },
    snapshot: {
      input: z
        .object({
          ids: z.array(z.guid()).optional(),
          at: z.iso.date().optional(),
        })
        .optional(),
      output: z.object({
        data: z.array(snapshotSchema),
      }),
    },
  },

  getBy: {
    id: {
      input: z.object({
        id: z.guid(),
      }),
      output: rowSchema,
    },
    tag: {
      input: z.object({
        tag: z.string().min(1).trim(),
      }),
      output: rowSchema,
    },
  },

  get: {
    snapshot: {
      input: z.object({
        id: z.guid(),
        at: z.iso.date().optional(),
      }),
      output: snapshotSchema,
    },
  },

  create: {
    input: z.object({
      tag: z.string().min(1, { message: "Informe a TAG do tanque" }).trim(),
      concession_id: z.guid(),
      installation_id: z.guid(),
      measurement_equipment_id: z.guid().nullable().optional(),
      latitude: latitudeSchema.nullable().optional(),
      longitude: longitudeSchema.nullable().optional(),
    }),
    output: rowSchema,
  },

  update: {
    input: z.object({
      id: z.guid(),
      tag: z.string().min(1).trim().optional(),
      concession_id: z.guid().optional(),
      installation_id: z.guid().optional(),
      measurement_equipment_id: z.guid().nullable().optional(),
      latitude: latitudeSchema.nullable().optional(),
      longitude: longitudeSchema.nullable().optional(),
    }),
    output: rowSchema,
  },

  delete: {
    input: z.object({
      ids: z.array(z.guid()).min(1),
    }),
    output: z.null(),
  },
};
