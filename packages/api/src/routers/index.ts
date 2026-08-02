import { commerceRouter } from "@lindaflor/api/routers/commerce";
import { healthRouter } from "@lindaflor/api/routers/health";
import type { RouterClient } from "@orpc/server";

type AppRouter = {
  health: typeof healthRouter;
  commerce: typeof commerceRouter;
};

export const appRouter: AppRouter = {
  health: healthRouter,
  commerce: commerceRouter,
};

export type AppRouterClient = RouterClient<AppRouter>;
