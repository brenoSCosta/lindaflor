import { Effect } from "effect";

import { coerceTimeFormatStr } from "@/context/time-format-options";
import type { StoredDefinition } from "@/lib/reports/registry";
import { getDefinition } from "@/lib/reports/registry";
import {
  buildReportRowsWithProgress,
  type PostProgress,
} from "@/lib/reports/render-helpers";
import type {
  OrgReportTheme,
  ReportErrorMessage,
  ReportFormatContext,
  ReportReadyMessage,
  SpawnReportWorkerMessage,
} from "@/lib/reports/types";

declare const self: {
  postMessage(message: unknown, transfer?: Transferable[]): void;
  addEventListener(
    type: "message",
    listener: (event: MessageEvent) => void,
  ): void;
};

type OutboundMessage =
  | ReportReadyMessage
  | ReportErrorMessage
  | { type: "progress"; percent: number; step: string };

function postToMain(message: OutboundMessage): void {
  self.postMessage(message);
}

function postProgress(percent: number, step: string): void {
  postToMain({ type: "progress", percent, step });
}

function postReady(blob: Blob): void {
  postToMain({ type: "ready", blob });
}

function postError(message: string): void {
  postToMain({ type: "error", message });
}

function buildFormatContext(
  message: SpawnReportWorkerMessage,
): ReportFormatContext {
  return {
    timezone: message.timezone,
    datetimeFormat: coerceTimeFormatStr(message.datetimeFormat),
  };
}

function buildRowsFromMessage(
  message: SpawnReportWorkerMessage,
  definition: StoredDefinition,
): string[][] {
  const ctx = buildFormatContext(message);
  return buildReportRowsWithProgress(
    message.rows,
    definition.columns,
    ctx,
    definition.format,
    postProgress satisfies PostProgress,
  );
}

export function runWorkerHandler(
  message: SpawnReportWorkerMessage,
  generate: (
    title: string,
    columns: StoredDefinition["columns"],
    rows: string[][],
    ctx: ReportFormatContext,
    orgTheme?: OrgReportTheme,
  ) => Effect.Effect<Blob, Error>,
): void {
  const definition = getDefinition(message.key);
  const ctx = buildFormatContext(message);
  const program = Effect.gen(function* () {
    postProgress(5, "Iniciando…");
    const reportRows = buildRowsFromMessage(message, definition);
    postProgress(60, "Gerando documento…");
    const blob = yield* generate(
      definition.title,
      definition.columns,
      reportRows,
      ctx,
      message.orgTheme,
    );
    postProgress(100, "Finalizando…");
    postReady(blob);
  });

  void Effect.runPromise(
    program.pipe(
      Effect.catchAll((error) => {
        postError(error.message);
        return Effect.void;
      }),
      Effect.catchAllDefect((defect) => {
        const msg =
          defect instanceof Error ? defect.message : "Falha ao gerar relatório";
        postError(msg);
        return Effect.void;
      }),
    ),
  );
}
