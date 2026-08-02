import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { AcceptInvitation } from "@/routes/-components/accept-invitation";

export const Route = createFileRoute("/accept-invitation")({
  validateSearch: z.object({
    id: z.string().min(1),
    email: z.email().optional(),
  }),
  component: AcceptInvitation,
});
