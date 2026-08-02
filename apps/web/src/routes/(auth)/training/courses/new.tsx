import { createFileRoute, redirect } from "@tanstack/react-router";

import { CreateCoursePage } from "@/routes/(auth)/training/-components/create-course-page";

export const Route = createFileRoute("/(auth)/training/courses/new")({
  component: CreateCoursePage,
  beforeLoad: ({ context }) => {
    if (!context.ability.can("manage", "Training")) {
      throw redirect({
        to: "/training",
        search: { pageIndex: 1, pageSize: 12 },
      });
    }
  },
});
