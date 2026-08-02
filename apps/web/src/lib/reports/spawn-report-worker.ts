import { Effect } from "effect";

import { withReportPermit } from "@/lib/reports/semaphore";
import type {
  ReportFormat,
  ReportWorkerMessage,
  SpawnReportWorkerMessage,
} from "@/lib/reports/types";

const WORKER_URLS = {
  pdf: new URL("./workers/pdf-worker.ts", import.meta.url),
  xlsx: new URL("./workers/xlsx-worker.ts", import.meta.url),
} as const satisfies Record<ReportFormat, URL>;

export class ReportAbortedError extends Error {
  override readonly name = "ReportAbortedError";
  constructor() {
    super("Relatório cancelado");
  }
}

export function spawnReportWorker(
  message: SpawnReportWorkerMessage,
  onProgress: (percent: number, step: string) => void,
  signal?: AbortSignal,
): Promise<Blob> {
  if (signal?.aborted) {
    return Promise.reject(new ReportAbortedError());
  }

  const program = withReportPermit(
    Effect.async<Blob, Error>((resume) => {
      const worker = new Worker(WORKER_URLS[message.format], {
        type: "module",
      });

      const cleanup = () => {
        worker.removeEventListener("message", handleMessage);
        worker.removeEventListener("error", handleError);
        if (signal) {
          signal.removeEventListener("abort", handleAbort);
        }
        worker.terminate();
      };

      const handleAbort = () => {
        cleanup();
        resume(Effect.fail(new ReportAbortedError()));
      };

      const handleMessage = (event: MessageEvent<ReportWorkerMessage>) => {
        const data = event.data;
        if (data.type === "progress") {
          onProgress(data.percent, data.step);
          return;
        }
        cleanup();
        if (data.type === "error") {
          resume(Effect.fail(new Error(data.message)));
        } else {
          resume(Effect.succeed(data.blob));
        }
      };

      const handleError = (ev: ErrorEvent) => {
        cleanup();
        resume(
          Effect.fail(
            ev.error instanceof Error ? ev.error : new Error(ev.message),
          ),
        );
      };

      worker.addEventListener("message", handleMessage);
      worker.addEventListener("error", handleError);
      if (signal) {
        signal.addEventListener("abort", handleAbort);
      }
      worker.postMessage(message, []);
    }),
  );

  return Effect.runPromise(program);
}
