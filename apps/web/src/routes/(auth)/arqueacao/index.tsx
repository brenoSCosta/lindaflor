import { createFileRoute, redirect } from "@tanstack/react-router";

import { ArqueacaoPage } from "@/routes/(auth)/arqueacao/-components/page";
import {
  arqueacaoSearchSchema,
  type ArqueacaoSearch,
} from "@/routes/(auth)/arqueacao/-components/search-schema";

export const Route = createFileRoute("/(auth)/arqueacao/")({
  validateSearch: (search: Record<string, unknown>): ArqueacaoSearch =>
    arqueacaoSearchSchema.parse(search),
  beforeLoad: ({ context }) => {
    if (!context.ability.can("read", "TankCalibrations")) {
      throw redirect({
        to: "/tankages",
      });
    }
  },
  component: ArqueacaoPage,
});
