import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

import { CheckEmail } from "@/routes/-components/check-email";

export const Route = createFileRoute("/check-email")({
  validateSearch: z.object({ email: z.email() }),
  component: CheckEmail,
});
