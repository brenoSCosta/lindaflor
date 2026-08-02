import { createFileRoute, redirect } from "@tanstack/react-router";

import { UpdateCoursePage } from "@/routes/(auth)/training/-components/update-course-page";

export const Route = createFileRoute("/(auth)/training/courses/$id/manage")({
  component: UpdateCoursePage,
  beforeLoad: ({ context }) => {
    if (!context.ability.can("manage", "Training")) {
      throw redirect({
        to: "/training",
        search: { pageIndex: 1, pageSize: 12 },
      });
    }
  },
});
