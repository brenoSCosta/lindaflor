import { createFileRoute } from "@tanstack/react-router";

import { LabAnalysesPage } from "@/routes/(auth)/tankages/-components/lab-analyses-page";

export const Route = createFileRoute(
  "/(auth)/tankages/$tankId/analises-laboratorio",
)({
  component: LabAnalysesRoute,
});

function LabAnalysesRoute() {
  const { tankId } = Route.useParams();
  return <LabAnalysesPage tankId={tankId} />;
}
