import { z } from "zod";

export const cadastrosTabValues = [
  "tanks",
  "concessions",
  "installations",
  "trenas",
] as const;

export const cadastrosSearchSchema = z.object({
  tab: z.enum(cadastrosTabValues).default("tanks"),
});

export type CadastrosSearch = z.infer<typeof cadastrosSearchSchema>;
