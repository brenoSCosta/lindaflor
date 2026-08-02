import { createFileRoute } from "@tanstack/react-router";

import { TrainingCatalogPage } from "@/routes/(auth)/training/-components/catalog-page";
import {
  trainingSearchSchema,
  type TrainingSearch,
} from "@/routes/(auth)/training/-components/search-schema";

export const Route = createFileRoute("/(auth)/training/")({
  validateSearch: (search: Record<string, unknown>): TrainingSearch =>
    trainingSearchSchema.parse(search),
  component: TrainingCatalogPage,
});
