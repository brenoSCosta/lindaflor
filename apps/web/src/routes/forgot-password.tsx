import { createFileRoute } from "@tanstack/react-router";

import { ForgotPassword } from "@/routes/-components/forgot-password";

export const Route = createFileRoute("/forgot-password")({
  component: ForgotPassword,
});
