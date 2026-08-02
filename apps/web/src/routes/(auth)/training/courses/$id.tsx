import { createFileRoute, Outlet } from "@tanstack/react-router";

export const Route = createFileRoute("/(auth)/training/courses/$id")({
  component: () => <Outlet />,
  loader: ({ params, context }) => ({
    data: context.orpc.training.v1.courses.get.queryOptions({
      input: { id: params.id },
    }),
  }),
  head: () => ({
    meta: [
      { title: "Curso - OG Service" },
      { name: "description", content: "Detalhes do curso de treinamento." },
    ],
  }),
});
