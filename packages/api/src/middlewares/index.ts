import type { Context } from "@lindaflor/api/context";
import { ratelimitMiddleware } from "@lindaflor/api/middlewares/rate-limit";
import { os } from "@orpc/server";

export const o = os.$context<Context>();

export const publicProcedure = o.use(ratelimitMiddleware).errors({
  TOO_MANY_REQUESTS: {
    message: "Muitas solicitações, tente novamente mais tarde",
  },
  PAYLOAD_TOO_LARGE: {
    message:
      "O corpo da solicitação excede o tamanho máximo permitido em bytes",
  },
  SERVICE_UNAVAILABLE: {
    message: "Serviço temporariamente indisponível",
  },
});
