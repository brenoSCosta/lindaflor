import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { CurriculumListPage } from "@/routes/(auth)/curriculum/-components/curriculum-list-page";

const searchSchema = z.object({
  search: z.string().optional().catch(undefined),
  pageIndex: z.coerce.number().min(1).catch(1),
});

export type CurriculumSearch = z.infer<typeof searchSchema>;

export const Route = createFileRoute("/(auth)/curriculum/")({
  validateSearch: (search: Record<string, unknown>): CurriculumSearch =>
    searchSchema.parse(search),
  component: CurriculumListPage,
  head: () => ({
    meta: [
      { title: "Banco de talentos - OG Service" },
      {
        name: "description",
        content: "Navegue pelos currículos enviados por candidatos.",
      },
    ],
  }),
});
