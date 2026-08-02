import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/(auth)/curriculum")({
  beforeLoad: ({ context }) => {
    const role = context.session.user.role;
    if (!role?.includes("admin") && !role?.includes("moderator")) {
      throw redirect({ to: "/curriculum/submit" });
    }
  },
});
