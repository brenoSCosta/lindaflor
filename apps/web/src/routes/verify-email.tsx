import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { VerifyEmail } from "@/routes/-components/verify-email";

export const Route = createFileRoute("/verify-email")({
  validateSearch: z.object({ error: z.string().optional() }),
  component: VerifyEmail,
});
