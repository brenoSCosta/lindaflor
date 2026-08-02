import { createFileRoute } from "@tanstack/react-router";

import { DashboardPage } from "@/routes/(auth)/dashboard/-components/page";

export const Route = createFileRoute("/(auth)/dashboard/")({
  component: DashboardPage,
});
