import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { ResetPassword } from "@/routes/-components/reset-password";

export const Route = createFileRoute("/reset-password")({
  validateSearch: z.object({ token: z.string().min(1) }),
  component: ResetPassword,
});
