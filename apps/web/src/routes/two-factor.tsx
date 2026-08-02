import { createFileRoute } from "@tanstack/react-router";

import { TwoFactor } from "@/routes/-components/two-factor";

export const Route = createFileRoute("/two-factor")({
  component: TwoFactor,
});
