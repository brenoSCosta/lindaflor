import { createFileRoute } from "@tanstack/react-router";

import { TankagesTankListPage } from "@/routes/(auth)/tankages/-components/tank-list-page";

export const Route = createFileRoute("/(auth)/tankages/")({
  component: TankagesTankListPage,
});
