import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";

/**
 * Shared oRPC procedure for endpoints that require "manage" on Training.
 * Used by courses and lectures routers.
 */
export const manageTrainingProcedure = authorizedProcedure
  .use(requireActiveOrganization())
  .use(authorize("manage", "Training"))
  .errors({
    FORBIDDEN: {
      message: "Você não tem permissão para executar esta ação",
    },
  });
