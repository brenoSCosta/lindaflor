import { createFileRoute, redirect } from "@tanstack/react-router";

import { CadastrosPage } from "@/routes/(auth)/cadastros/-components/page";
import {
  cadastrosSearchSchema,
  type CadastrosSearch,
} from "@/routes/(auth)/cadastros/-components/search-schema";

export const Route = createFileRoute("/(auth)/cadastros/")({
  validateSearch: (search: Record<string, unknown>): CadastrosSearch =>
    cadastrosSearchSchema.parse(search),
  beforeLoad: ({ context }) => {
    if (!context.ability.can("manage", "Tanks")) {
      throw redirect({
        to: "/tankages",
      });
    }
  },
  component: CadastrosPage,
});
