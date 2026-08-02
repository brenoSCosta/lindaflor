import type { Column, Row, RowData } from "@tanstack/react-table";

import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import { getColumnLabel } from "@/components/ui/data-table/helpers/column-label";
import { ROW_ACTIONS_COLUMN_ID } from "@/components/ui/data-table/injected-columns/data-table-row-actions";
import { EXPAND_COLUMN_ID } from "@/components/ui/data-table/injected-columns/expand-column";
import { ROW_DRAG_COLUMN_ID } from "@/components/ui/data-table/injected-columns/row-drag-column";
import { ROW_NUMBER_COLUMN_ID } from "@/components/ui/data-table/injected-columns/row-number-column";
import { SELECTION_COLUMN_ID } from "@/components/ui/data-table/injected-columns/selection-column";
import type { ReportColumn } from "@/components/ui/data-table/reports/types";

const NON_DATA_COLUMNS = new Set([
  SELECTION_COLUMN_ID,
  ROW_NUMBER_COLUMN_ID,
  ROW_DRAG_COLUMN_ID,
  EXPAND_COLUMN_ID,
  ROW_ACTIONS_COLUMN_ID,
]);

/** Which rows to export. */
export type ExportScope = "selected" | "filtered" | "all" | "page";

export interface ExportOptions {
  fileName?: string;
  /** Defaults to "selected" when rows are selected, else "filtered". */
  scope?: ExportScope;
}

/** Payload ready for {@link useReportSwarm} enqueue. */
export interface TableReportPayload {
  readonly title: string;
  readonly filenameStem: string;
  readonly columns: readonly ReportColumn[];
  readonly rows: string[][];
}

/** Data columns suitable for export (excludes the injected display columns). */
export function getExportableColumns<TData extends RowData>(
  table: DataTableInstance<TData>,
): Column<TData>[] {
  return table
    .getVisibleLeafColumns()
    .filter(
      (column) => column.accessorFn != null && !NON_DATA_COLUMNS.has(column.id),
    );
}

function resolveRows<TData extends RowData>(
  table: DataTableInstance<TData>,
  scope: ExportScope,
): Row<TData>[] {
  switch (scope) {
    case "selected":
      return table.getSelectedRowModel().rows;
    case "page":
      return table.getRowModel().rows;
    case "all":
      return table.getPreFilteredRowModel().rows;
    default:
      return table.getFilteredRowModel().rows;
  }
}

function effectiveScope<TData extends RowData>(
  table: DataTableInstance<TData>,
  scope?: ExportScope,
): ExportScope {
  if (scope) return scope;
  return table.getSelectedRowModel().rows.length > 0 ? "selected" : "filtered";
}

function normalize(value: unknown): string {
  if (value == null) return "";
  if (
    typeof value === "number" ||
    typeof value === "boolean" ||
    typeof value === "string"
  ) {
    return String(value);
  }
  if (value instanceof Date) return value.toISOString();
  if (typeof value === "object") return JSON.stringify(value);
  return "";
}

/**
 * Builds an inline report payload from the current table state.
 * `fileName` (or `table.tableInstance.exportFileName`) is used as both the
 * document title and the download filename stem.
 */
export function buildTableReportPayload<TData extends RowData>(
  table: DataTableInstance<TData>,
  options: ExportOptions = {},
): TableReportPayload {
  const filenameStem =
    options.fileName ?? table.tableInstance.exportFileName ?? "export";
  const scope = effectiveScope(table, options.scope);
  const exportable = getExportableColumns(table);
  const columns: ReportColumn[] = exportable.map((column) => ({
    id: column.id,
    label: getColumnLabel(column),
  }));
  const rows = resolveRows(table, scope).map((row) =>
    exportable.map((column) => normalize(row.getValue(column.id))),
  );
  return {
    title: filenameStem,
    filenameStem,
    columns,
    rows,
  };
}
