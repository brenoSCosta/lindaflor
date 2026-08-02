import { createFileRoute, redirect } from "@tanstack/react-router";

import { ManagePage } from "@/routes/(auth)/training/-components/manage-page";
import {
  trainingManageSearchSchema,
  type TrainingManageSearch,
} from "@/routes/(auth)/training/-components/search-schema";

export const Route = createFileRoute("/(auth)/training/manage")({
  validateSearch: (search: Record<string, unknown>): TrainingManageSearch =>
    trainingManageSearchSchema.parse(search),
  component: ManagePage,
  beforeLoad: ({ context }) => {
    if (!context.ability.can("manage", "Training")) {
      throw redirect({
        to: "/training",
        search: { pageIndex: 1, pageSize: 12 },
      });
    }
  },
});
