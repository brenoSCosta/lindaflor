import { createFileRoute } from "@tanstack/react-router";

import { TankDaysPage } from "@/routes/(auth)/tankages/-components/tank-days-page";

export const Route = createFileRoute("/(auth)/tankages/$tankId/")({
  component: TankDaysRoute,
});

function TankDaysRoute() {
  const { tankId } = Route.useParams();
  return <TankDaysPage tankId={tankId} />;
}
