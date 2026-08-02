import { createFileRoute, redirect } from "@tanstack/react-router";
import { z } from "zod";

import { Login } from "@/routes/-components/login";

export const Route = createFileRoute("/login")({
  validateSearch: z.object({
    inviteId: z.string().min(1).optional(),
    email: z.email().optional(),
    mode: z.enum(["signin", "signup"]).optional(),
  }),
  beforeLoad: async ({ context }) => {
    const result = await context.auth.getSession();
    if (result.data) {
      throw redirect({ to: "/dashboard" });
    }
  },
  component: Login,
});
