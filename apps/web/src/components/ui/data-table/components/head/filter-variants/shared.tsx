import type { Column, RowData } from "@tanstack/react-table";
import React from "react";

import type {
  DataTableFilterOption,
  DataTableInstance,
} from "@/components/ui/data-table/core/types";

export interface FilterFieldProps<TData extends RowData, TValue> {
  column: Column<TData, TValue>;
  table: DataTableInstance<TData>;
}

export const FIELD_CLASS =
  "h-8 rounded-sm text-xs font-normal tracking-normal normal-case";

export const BETWEEN_MODES = new Set(["between", "betweenInclusive"]);

/**
 * Shared Calendar props so the month/year dropdowns are usable regardless of
 * which base Calendar component the consumer ships. The year dropdown derives
 * its options from the startMonth/endMonth range.
 */
export const CALENDAR_NAV_PROPS = {
  captionLayout: "dropdown",
  startMonth: new Date(new Date().getFullYear() - 100, 0),
  endMonth: new Date(new Date().getFullYear() + 10, 11),
} as const;

/** A muted pill shown for valueless modes (empty / not empty). */
export function ValuelessLabel({ label }: { label: string }) {
  return (
    <div className="flex h-8 items-center rounded-sm border border-dashed px-2 text-xs text-muted-foreground">
      {label}
    </div>
  );
}

/** Options for select-style variants: explicit `meta.options` or faceted values. */
export function useSelectOptions<TData extends RowData, TValue>(
  column: Column<TData, TValue>,
): { options: DataTableFilterOption[]; counts: Map<string, number> } {
  const facets = column.getFacetedUniqueValues();
  return React.useMemo(() => {
    const counts = new Map<string, number>();
    for (const [value, count] of facets) {
      if (value == null) continue;
      counts.set(String(value), count);
    }
    const explicit = column.columnDef.meta?.options;
    if (explicit && explicit.length > 0) {
      return { options: explicit, counts };
    }
    const options = Array.from(counts.keys())
      .toSorted((a, b) => a.localeCompare(b))
      .map((value) => ({ label: value, value }));
    return { options, counts };
  }, [facets, column.columnDef.meta?.options]);
}
