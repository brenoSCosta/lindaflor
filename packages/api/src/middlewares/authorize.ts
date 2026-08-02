import { oAuthorized } from "@lindaflor/api/middlewares/authorized";
import {
  abilityCannot,
  type ActionsBySubject,
} from "@lindaflor/shared/lib/ability/subjects";
import { ORPCError } from "@orpc/server";

export const authorize = <K extends keyof ActionsBySubject>(
  action: ActionsBySubject[K],
  subjectName: K,
) =>
  oAuthorized.middleware(async ({ context, next }) => {
    if (abilityCannot(context.ability, action, subjectName)) {
      throw new ORPCError("FORBIDDEN", {
        message: "Você não tem permissão para executar esta ação",
      });
    }
    return next({ context });
  });
