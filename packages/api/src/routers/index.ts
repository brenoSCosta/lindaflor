import { commerceRouter } from "@lindaflor/api/routers/commerce";
import { healthRouter } from "@lindaflor/api/routers/health";
import { organizationRouter } from "@lindaflor/api/routers/organization";
import { privateData } from "@lindaflor/api/routers/private-data";
import { userRouter } from "@lindaflor/api/routers/user";
import type { RouterClient } from "@orpc/server";

type AppRouter = {
  health: typeof healthRouter;
  user: typeof userRouter;
  commerce: typeof commerceRouter;
  organization: typeof organizationRouter;
  privateData: typeof privateData;
};

export const appRouter: AppRouter = {
  health: healthRouter,
  commerce: commerceRouter,
  user: userRouter,
  organization: organizationRouter,
  privateData,
};

export type AppRouterClient = RouterClient<AppRouter>;
