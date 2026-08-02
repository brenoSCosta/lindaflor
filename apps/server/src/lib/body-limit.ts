import type { AppRouterClient } from "@lindaflor/api/routers";
import type { Context } from "@orpc/server";
import { ORPCError } from "@orpc/server";
import type {
  FetchHandlerOptions,
  FetchHandlerPlugin,
} from "@orpc/server/fetch";

type RouterProcedurePaths<T> = {
  [K in keyof T & string]: T[K] extends Function
    ? K
    : T[K] extends Record<string, unknown>
      ? `${K}.${RouterProcedurePaths<T[K]>}`
      : never;
}[keyof T & string];

export type AppRouterProcedurePath = RouterProcedurePaths<AppRouterClient>;

export interface BodyLimitOptions<TProcedurePath extends string = string> {
  defaultMaxBodySize: number;
  overrides?: Partial<Record<TProcedurePath, number>>;
}

export class BodyLimitPlugin<
  T extends Context,
> implements FetchHandlerPlugin<T> {
  private readonly defaultMaxBodySize: number;
  private readonly pathOverrides: Partial<Record<string, number>>;

  constructor(options: BodyLimitOptions) {
    this.defaultMaxBodySize = options.defaultMaxBodySize;
    this.pathOverrides = options.overrides ?? {};
  }

  initRuntimeAdapter(handlerOptions: FetchHandlerOptions<T>): void {
    handlerOptions.adapterInterceptors ??= [];

    handlerOptions.adapterInterceptors.push(async (options) => {
      const bodyStream = options.request.body;
      if (!bodyStream) {
        return options.next();
      }

      const url = new URL(options.request.url);
      const pathname = url.pathname;
      const normalizedPrefix = (options.prefix ?? "").replace(/\/$/, "");

      let maxBodySize = this.defaultMaxBodySize;
      for (const [procPath, limit] of Object.entries(this.pathOverrides)) {
        if (limit === undefined) {
          continue;
        }

        const urlSegment = "/" + procPath.replace(/\./g, "/");
        const fullUrlPath = normalizedPrefix + urlSegment;

        if (
          pathname === fullUrlPath ||
          pathname.startsWith(fullUrlPath + "/")
        ) {
          maxBodySize = limit;
          break;
        }
      }

      let currentBodySize = 0;
      const reader = new ReadableStream({
        start: async (controller) => {
          try {
            if (
              Number(options.request.headers.get("content-length")) >
              maxBodySize
            ) {
              controller.error(
                new ORPCError("PAYLOAD_TOO_LARGE", {
                  message: `Request body exceeds the maximum allowed size of ${maxBodySize} bytes`,
                }),
              );
              return;
            }

            for await (const chunk of bodyStream) {
              currentBodySize += chunk.length;
              if (currentBodySize > maxBodySize) {
                controller.error(
                  new ORPCError("PAYLOAD_TOO_LARGE", {
                    message: `Request body exceeds the maximum allowed size of ${maxBodySize} bytes`,
                  }),
                );
                break;
              }
              controller.enqueue(chunk);
            }
          } finally {
            controller.close();
          }
        },
      });

      const requestInit: RequestInit & { duplex: "half" } = {
        body: reader,
        duplex: "half",
      };

      return options.next({
        ...options,
        request: new Request(options.request, requestInit),
      });
    });
  }
}
