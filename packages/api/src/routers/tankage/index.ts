import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { bulletinRouter } from "@lindaflor/api/routers/tankage/day-bulletins";
import { summaryRouter } from "@lindaflor/api/routers/tankage/day-summaries";
import { calibrationRouter } from "@lindaflor/api/routers/tankage/tank-calibrations";
import { tankageRouter } from "@lindaflor/api/routers/tankage/tankages";
import { tankRouter } from "@lindaflor/api/routers/tankage/tanks";
import { transferRouter } from "@lindaflor/api/routers/tankage/transfers";
import type { EnhancedRouter } from "@orpc/server";

type TanksV1Routes = {
  tank: typeof tankRouter;
  tankage: typeof tankageRouter;
  transfer: typeof transferRouter;
  calibration: typeof calibrationRouter;
  bulletin: typeof bulletinRouter;
  summary: typeof summaryRouter;
};

type TanksV1Router = EnhancedRouter<
  TanksV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type TanksRouter = {
  v1: TanksV1Router;
};

function createTanksV1Router(routes: TanksV1Routes): TanksV1Router {
  return o.prefix("/v1").tag("Tankage").router(routes);
}

export const tanksRouter: TanksRouter = {
  v1: createTanksV1Router({
    tank: tankRouter,
    tankage: tankageRouter,
    transfer: transferRouter,
    calibration: calibrationRouter,
    bulletin: bulletinRouter,
    summary: summaryRouter,
  }),
};
