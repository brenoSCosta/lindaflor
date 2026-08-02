import { createFileRoute } from "@tanstack/react-router";

import { orpc } from "@/lib/orpc";
import { CurriculumDetailPage } from "@/routes/(auth)/curriculum/-components/curriculum-detail-page";

export const Route = createFileRoute("/(auth)/curriculum/$id")({
  component: CurriculumDetailPage,
  loader: ({ params }) => ({
    data: orpc.curriculum.v1.getById.queryOptions({
      input: { id: params.id },
    }),
  }),
  head: () => ({
    meta: [
      { title: "Currículo - OG Service" },
      {
        name: "description",
        content: "Detalhes do currículo enviado.",
      },
    ],
  }),
});
