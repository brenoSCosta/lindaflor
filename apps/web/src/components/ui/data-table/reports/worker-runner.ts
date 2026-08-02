import { Effect } from "effect";

import type {
  OrgReportTheme,
  ReportColumn,
  ReportErrorMessage,
  ReportFormatContext,
  ReportReadyMessage,
  SpawnReportWorkerMessage,
} from "@/components/ui/data-table/reports/types";
import { coerceTimeFormatStr } from "@/context/time-format-options";

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

export function runWorkerHandler(
  message: SpawnReportWorkerMessage,
  generate: (
    title: string,
    columns: ReadonlyArray<ReportColumn>,
    rows: string[][],
    ctx: ReportFormatContext,
    orgTheme?: OrgReportTheme,
  ) => Effect.Effect<Blob, Error>,
): void {
  const ctx = buildFormatContext(message);
  const program = Effect.gen(function* () {
    postProgress(5, "Iniciando…");
    const columns = message.columns;
    const rows = message.rows.map((row) => [...row]);
    postProgress(55, "Dados prontos…");
    postProgress(60, "Gerando documento…");
    const blob = yield* generate(
      message.title,
      columns,
      rows,
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
