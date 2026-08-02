import { Effect } from "effect";

import { generateXlsxBlob } from "@/components/ui/data-table/reports/render-xlsx";
import type { SpawnReportWorkerMessage } from "@/components/ui/data-table/reports/types";
import { runWorkerHandler } from "@/components/ui/data-table/reports/worker-runner";

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
