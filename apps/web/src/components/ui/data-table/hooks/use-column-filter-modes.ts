import type { ColumnDef, FilterFn, RowData } from "@tanstack/react-table";
import React from "react";

import { createDynamicFilterFn } from "@/components/ui/data-table/fns/filter-factories";
import type { FilterMode } from "@/components/ui/data-table/fns/filter-modes";
import { defaultModeForVariant } from "@/components/ui/data-table/fns/variant-modes";
import { columnKey } from "@/components/ui/data-table/helpers/column-key";

export interface ColumnFilterModes<TData extends RowData> {
  columnFilterModes: Record<string, FilterMode>;
  setColumnFilterModes: React.Dispatch<
    React.SetStateAction<Record<string, FilterMode>>
  >;
  /** Resolve the active mode for a column (active → default → "contains"). */
  getColumnMode: (columnId: string) => FilterMode;
  /** Single dynamic filter fn assigned to every column via `defaultColumn`. */
  dynamicFilterFn: FilterFn<TData>;
}

/**
 * Per-column filter-mode state and the dynamic filter function that reads it.
 * Default modes are derived from each column's `meta.filterMode` / `meta.variant`.
 * Modes are read through refs so the filter fn keeps a stable identity (changing
 * it would thrash the filtered row model every render).
 *
 * The value-resetting `setColumnFilterMode` action lives in `useDataTable`
 * because it needs the table instance, which can't exist before this hook
 * supplies `dynamicFilterFn`.
 */
export function useColumnFilterModes<TData extends RowData>(
  columns: ColumnDef<TData>[],
): ColumnFilterModes<TData> {
  const [columnFilterModes, setColumnFilterModes] = React.useState<
    Record<string, FilterMode>
  >({});

  // Per-column default mode derived from `meta.filterMode` / `meta.variant`.
  const defaultModes = React.useMemo(() => {
    const map: Record<string, FilterMode> = {};
    for (const def of columns) {
      const key = columnKey(def);
      if (!key) continue;
      const meta = def.meta;
      map[key] =
        meta?.filterMode ?? defaultModeForVariant(meta?.variant ?? "text");
    }
    return map;
  }, [columns]);

  const getColumnMode = React.useCallback(
    (columnId: string): FilterMode =>
      columnFilterModes[columnId] ?? defaultModes[columnId] ?? "contains",
    [columnFilterModes, defaultModes],
  );

  const dynamicFilterFn = React.useMemo(
    () => createDynamicFilterFn<TData>(getColumnMode),
    [getColumnMode],
  );

  return {
    columnFilterModes,
    setColumnFilterModes,
    getColumnMode,
    dynamicFilterFn,
  };
}
