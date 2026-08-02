import { contract } from "@lindaflor/api/contract/index.gen";
import type { AppRouterClient } from "@lindaflor/api/routers";
import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import { SimpleCsrfProtectionLinkPlugin } from "@orpc/client/plugins";
import { inferRPCMethodFromContractRouter } from "@orpc/contract";
import { createTanstackQueryUtils } from "@orpc/tanstack-query";
import { QueryCache, QueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { getApiBaseUrl } from "@/lib/api-base-url";

export const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (error) => {
      toast.error(`Error: ${error.message}`);
    },
  }),
});

export const link = new RPCLink({
  url: `${getApiBaseUrl()}/rpc`,
  method: inferRPCMethodFromContractRouter(contract),
  plugins: [new SimpleCsrfProtectionLinkPlugin()],
  fetch(url, options) {
    return fetch(url, {
      ...options,
      credentials: "include",
    });
  },
});

const client: AppRouterClient = createORPCClient(link);

export const orpc = createTanstackQueryUtils(client);
