import type { Column, RowData } from "@tanstack/react-table";

import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import type { FilterMode } from "@/components/ui/data-table/fns/filter-modes";
import { defaultModeForVariant } from "@/components/ui/data-table/fns/variant-modes";

/** Effective filter mode for a column: explicit selection → meta → variant default. */
export function getEffectiveMode<TData extends RowData, TValue>(
  column: Column<TData, TValue>,
  table: DataTableInstance<TData>,
): FilterMode {
  const variant = column.columnDef.meta?.variant ?? "text";
  return (
    table.tableInstance.columnFilterModes[column.id] ??
    column.columnDef.meta?.filterMode ??
    defaultModeForVariant(variant)
  );
}
