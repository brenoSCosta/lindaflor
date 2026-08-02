import {
  audit_aggregate_types,
  audit_entity_types,
  audit_event_actions,
} from "@lindaflor/shared/enums/audit";
import { tank_day_bulletin_statuses } from "@lindaflor/shared/enums/tankage";
import { z } from "zod";

const statusSchema = z.enum(tank_day_bulletin_statuses);

const outputSchema = z.object({
  id: z.guid().nullable(),
  tank_id: z.guid(),
  operational_day: z.iso.date(),
  status: statusSchema,
  approved_at: z.date().nullable(),
  approved_by_user_id: z.guid().nullable(),
  approved_by_name: z.string().nullable(),
  reopened_at: z.date().nullable(),
  reopened_by_user_id: z.guid().nullable(),
  reopened_by_name: z.string().nullable(),
  organization_id: z.guid(),
});
export type TankDayBulletinOutput = z.infer<typeof outputSchema>;

const auditChangeSchema = z.object({
  field: z.string(),
  from: z.union([z.string(), z.number(), z.null()]),
  to: z.union([z.string(), z.number(), z.null()]),
});

const auditEventMetadataSchema = z
  .object({
    changes: z.array(auditChangeSchema).optional(),
    justification: z.string().optional(),
    deleted_count: z.number().int().nonnegative().optional(),
    deleted_ids: z.array(z.guid()).optional(),
    current_measurement: z.number().optional(),
    measured_at: z.string().optional(),
    height_before_m: z.number().optional(),
    height_after_m: z.number().optional(),
    gross_volume_out_m3: z.number().optional(),
    destination_label: z.string().nullable().optional(),
    tankage_id: z.guid().optional(),
  })
  .nullable();

const auditEventOutputSchema = z.object({
  id: z.guid(),
  entity_type: z.enum(audit_entity_types),
  action: z.enum(audit_event_actions),
  entity_id: z.guid().nullable(),
  actor_user_id: z.guid().nullable(),
  actor_name: z.string(),
  occurred_at: z.date(),
  metadata: auditEventMetadataSchema,
  aggregate_type: z.enum(audit_aggregate_types),
});
export type TankDayBulletinAuditEventOutput = z.infer<
  typeof auditEventOutputSchema
>;

export const schema = {
  getBy: {
    day: {
      input: z.object({
        tank_id: z.guid(),
        operational_day: z.iso.date(),
      }),
      output: outputSchema,
    },
  },

  listBy: {
    tank: {
      input: z.object({
        tank_id: z.guid(),
      }),
      output: z.object({
        data: z.array(
          z.object({
            operational_day: z.iso.date(),
            status: statusSchema,
          }),
        ),
      }),
    },
  },

  event: {
    listBy: {
      day: {
        input: z.object({
          tank_id: z.guid(),
          operational_day: z.iso.date(),
        }),
        output: z.object({
          data: z.array(auditEventOutputSchema),
        }),
      },
    },
  },

  approve: {
    input: z.object({
      tank_id: z.guid(),
      operational_day: z.iso.date(),
    }),
    output: outputSchema,
  },

  reopen: {
    input: z.object({
      tank_id: z.guid(),
      operational_day: z.iso.date(),
    }),
    output: outputSchema,
  },

  delete: {
    input: z.object({
      tank_id: z.guid(),
      operational_day: z.iso.date(),
    }),
    output: z.null(),
  },
};
