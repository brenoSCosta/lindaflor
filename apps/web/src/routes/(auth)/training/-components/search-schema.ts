import { schema } from "@lindaflor/shared/schemas/training";
import { z } from "zod";

export const trainingTabSchema = z.enum(["my", "available"]).catch("my");

const trainingBaseSearchSchema = z.object({
  pageIndex: z.coerce.number().int().min(1).catch(1),
  pageSize: z.coerce.number().int().min(1).max(50).catch(12),
  search: schema.v1.courses.list.input.shape.search,
});

export const trainingSearchSchema = trainingBaseSearchSchema.extend({
  tab: trainingTabSchema.optional().catch(undefined),
});

export type TrainingSearch = z.infer<typeof trainingSearchSchema>;

export const trainingManageSearchSchema = trainingBaseSearchSchema;

export type TrainingManageSearch = z.infer<typeof trainingManageSearchSchema>;
