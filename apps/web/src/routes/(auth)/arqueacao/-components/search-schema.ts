import { z } from "zod";

export const arqueacaoSearchSchema = z.object({
  tank_id: z.guid().optional().catch(undefined),
});

export type ArqueacaoSearch = z.infer<typeof arqueacaoSearchSchema>;
