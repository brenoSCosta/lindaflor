import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { coursesRouter } from "@lindaflor/api/routers/training/courses";
import { enrollmentsRouter } from "@lindaflor/api/routers/training/enrollments";
import { lecturesRouter } from "@lindaflor/api/routers/training/lectures";
import { quizzesRouter } from "@lindaflor/api/routers/training/quizzes";
import type { EnhancedRouter } from "@orpc/server";

type TrainingV1Routes = {
  courses: typeof coursesRouter;
  enrollments: typeof enrollmentsRouter;
  lectures: typeof lecturesRouter;
  quizzes: typeof quizzesRouter;
};

type TrainingV1Router = EnhancedRouter<
  TrainingV1Routes,
  Context,
  Context,
  Record<never, never>
>;

type TrainingRouter = {
  v1: TrainingV1Router;
};

function createTrainingV1Router(routes: TrainingV1Routes): TrainingV1Router {
  return o.prefix("/v1").tag("Training").router(routes);
}

export const trainingRouter: TrainingRouter = {
  v1: createTrainingV1Router({
    courses: coursesRouter,
    enrollments: enrollmentsRouter,
    lectures: lecturesRouter,
    quizzes: quizzesRouter,
  }),
};
