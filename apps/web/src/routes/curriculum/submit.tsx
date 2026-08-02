import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { orpc } from "@/lib/orpc";
import { CurriculumSubmitPage } from "@/routes/curriculum/-components/curriculum-submit-page";

const searchSchema = z.object({
  career: z.guid().optional(),
});

export const Route = createFileRoute("/curriculum/submit")({
  component: CurriculumSubmitPage,
  validateSearch: (search) => searchSchema.parse(search),
  loader: () => ({
    careers: orpc.career.v1.list.queryOptions({
      input: { onlyActive: true },
    }),
  }),
  head: () => ({
    meta: [
      { title: "Banco de talentos - OG Service" },
      {
        name: "description",
        content:
          "Cadastre-se no banco de talentos da OG Service e candidate-se às vagas abertas.",
      },
    ],
  }),
});
