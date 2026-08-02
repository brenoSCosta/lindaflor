import { createFileRoute } from "@tanstack/react-router";

import { TankDayBulletinPage } from "@/routes/(auth)/tankages/-components/tank-day-bulletin-page";

export const Route = createFileRoute("/(auth)/tankages/$tankId/$date")({
  component: TankDayBulletinRoute,
});

function TankDayBulletinRoute() {
  const { tankId, date } = Route.useParams();
  return <TankDayBulletinPage tankId={tankId} date={date} />;
}
