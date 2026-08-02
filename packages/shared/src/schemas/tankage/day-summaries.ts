import { tank_day_bulletin_statuses } from "@lindaflor/shared/enums/tankage";
import { z } from "zod";

const bulletinStatusSchema = z.enum(tank_day_bulletin_statuses);

const summaryRowSchema = z.object({
  operational_day: z.iso.date(),
  measurement_count: z.number().int().nonnegative(),
  last_measured_at: z.date(),
  last_current_measurement: z.number(),
  last_operator_name: z.string(),
  bulletin_status: bulletinStatusSchema,
  production_gross_volume_m3: z.number().nullable(),
  production_net_oil_volume_m3_20c: z.number().nullable(),
  production_volume_oil_barrels: z.number().nullable(),
});

export type TankDaySummaryOutput = z.infer<typeof summaryRowSchema>;

export const schema = {
  listBy: {
    tank: {
      input: z.object({
        tank_id: z.guid(),
      }),
      output: z.object({
        data: z.array(summaryRowSchema),
      }),
    },
  },
};
