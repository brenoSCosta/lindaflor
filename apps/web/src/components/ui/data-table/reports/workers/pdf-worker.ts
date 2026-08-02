import { generatePdfBlob } from "@/components/ui/data-table/reports/render-pdf";
import type { SpawnReportWorkerMessage } from "@/components/ui/data-table/reports/types";
import { runWorkerHandler } from "@/components/ui/data-table/reports/worker-runner";

self.addEventListener(
  "message",
  (event: MessageEvent<SpawnReportWorkerMessage>) => {
    runWorkerHandler(event.data, (title, columns, rows, ctx, orgTheme) =>
      generatePdfBlob(title, columns, rows, ctx, orgTheme),
    );
  },
);
