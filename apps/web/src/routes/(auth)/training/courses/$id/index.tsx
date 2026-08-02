import { createFileRoute } from "@tanstack/react-router";

import { TrainingCoursePage } from "@/routes/(auth)/training/-components/course-page";

export const Route = createFileRoute("/(auth)/training/courses/$id/")({
  component: TrainingCoursePage,
});
