import { z } from "zod";

export const rowSchema = z.object({
  id: z.guid(),
  organization_id: z.guid(),
  user_id: z.guid(),
  role: z.string(),
  created_at: z.date(),
});
export type MemberOutput = z.infer<typeof rowSchema>;
