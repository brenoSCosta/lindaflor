import { generatePdfBlob } from "@/lib/reports/render-pdf";
import type { SpawnReportWorkerMessage } from "@/lib/reports/types";
import { runWorkerHandler } from "@/lib/reports/worker-runner";
import { registerTodoReport } from "@/routes/dev/todos/-components/todo-report-definition";

registerTodoReport();

self.addEventListener(
  "message",
  (event: MessageEvent<SpawnReportWorkerMessage>) => {
    runWorkerHandler(event.data, (title, columns, rows, ctx, orgTheme) =>
      generatePdfBlob(title, columns, rows, ctx, orgTheme),
    );
  },
);
