import { Effect } from "effect";

import { generateXlsxBlob } from "@/lib/reports/render-xlsx";
import type { SpawnReportWorkerMessage } from "@/lib/reports/types";
import { runWorkerHandler } from "@/lib/reports/worker-runner";
import { registerTodoReport } from "@/routes/dev/todos/-components/todo-report-definition";

registerTodoReport();

self.addEventListener(
  "message",
  (event: MessageEvent<SpawnReportWorkerMessage>) => {
    runWorkerHandler(event.data, (title, columns, rows) =>
      Effect.try({
        try: () => generateXlsxBlob(title, columns, rows),
        catch: (error) =>
          error instanceof Error ? error : new Error("Falha ao gerar XLSX"),
      }),
    );
  },
);
