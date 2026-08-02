import type { RowData } from "@tanstack/react-table";
import React from "react";

import { Button } from "@/components/ui/button";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import type { ReportFormat } from "@/components/ui/data-table/reports/types";
import {
  ReportProgressBar,
  useReportSwarm,
} from "@/components/ui/data-table/reports/use-report-swarm";
import { buildTableReportPayload } from "@/components/ui/data-table/utils/export-utils";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

const FORMAT_EXTENSION: Record<ReportFormat, string> = {
  csv: "csv",
  xlsx: "xlsx",
  pdf: "pdf",
};

/**
 * Toolbar export menu (CSV / Excel / PDF). Runs generation in the report worker
 * swarm. Exports the selected rows when any are selected, otherwise the full
 * filtered set across all pages. Document title and filename stem come from
 * `exportFileName` (or the `fileName` prop).
 */
export function DataTableExportMenu<TData extends RowData>({
  table,
  fileName,
}: {
  table: DataTableInstance<TData>;
  fileName?: string;
}) {
  const { localization, icons } = table.tableInstance;
  const { enqueue, jobs } = useReportSwarm();

  const busyFormats = React.useMemo(() => {
    const formats = new Set<ReportFormat>();
    for (const job of jobs.values()) {
      formats.add(job.format);
    }
    return formats;
  }, [jobs]);

  const handleExport = React.useCallback(
    (format: ReportFormat) => {
      const payload = buildTableReportPayload(table, { fileName });
      enqueue({
        format,
        filename: `${payload.filenameStem}.${FORMAT_EXTENSION[format]}`,
        title: payload.title,
        columns: payload.columns,
        fetchRows: () => Promise.resolve(payload.rows),
      });
    },
    [table, fileName, enqueue],
  );

  return (
    <div className="flex flex-row items-end gap-1.5">
      <DropdownMenu>
        <Tooltip>
          <TooltipTrigger
            render={
              <DropdownMenuTrigger
                render={
                  <Button
                    variant="outline"
                    size="icon"
                    aria-label={localization.export}
                    className="size-8"
                  />
                }
              />
            }
          >
            <icons.export />
          </TooltipTrigger>
          <TooltipContent>{localization.export}</TooltipContent>
        </Tooltip>
        <DropdownMenuContent align="end" className="w-48">
          <DropdownMenuItem
            disabled={busyFormats.has("csv")}
            onClick={() => handleExport("csv")}
          >
            <icons.fileCsv />
            {localization.exportCsv}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={busyFormats.has("xlsx")}
            onClick={() => handleExport("xlsx")}
          >
            <icons.fileExcel />
            {localization.exportExcel}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={busyFormats.has("pdf")}
            onClick={() => handleExport("pdf")}
          >
            <icons.filePdf />
            {localization.exportPdf}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {[...jobs.values()].map((job) => (
        <ReportProgressBar
          key={job.id}
          percent={job.percent}
          step={job.step}
          label={job.format.toUpperCase()}
          className="w-48"
        />
      ))}
    </div>
  );
}
