import { createFileRoute } from "@tanstack/react-router";

import { TrainingCertificatePage } from "@/routes/(auth)/training/-components/certificate-page";

export const Route = createFileRoute(
  "/(auth)/training/courses/$id/certificate",
)({
  component: TrainingCertificatePage,
});
