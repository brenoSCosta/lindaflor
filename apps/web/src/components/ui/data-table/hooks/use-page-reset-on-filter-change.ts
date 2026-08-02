import type { RowData, Table } from "@tanstack/react-table";
import { useState } from "react";

import type { AdvancedFilterGroup } from "@/components/ui/data-table/core/types";

interface PageResetParams {
  enablePagination: boolean;
  manualPagination?: boolean;
  autoResetPageIndex?: boolean;
  enableAdvancedFilter?: boolean;
  advancedFilter?: AdvancedFilterGroup;
}

/**
 * Clamps to the first page when the filter set changes (MRT behaviour),
 * replacing TanStack's render-phase auto-reset (which warns in React 19 dev).
 * Runs after mount so there is no state update during render. Skipped under
 * manual pagination (server owns it) and when the consumer opted into the
 * native auto-reset.
 */
export function usePageResetOnFilterChange<TData extends RowData>(
  table: Table<TData>,
  {
    enablePagination,
    manualPagination,
    autoResetPageIndex,
    enableAdvancedFilter,
    advancedFilter,
  }: PageResetParams,
): void {
  const currentFilters = JSON.stringify(table.getState().columnFilters);
  const currentGlobalFilter = table.getState().globalFilter;
  const currentAdvancedFilter = JSON.stringify(advancedFilter);

  const [prevFilters, setPrevFilters] = useState(currentFilters);
  const [prevGlobalFilter, setPrevGlobalFilter] = useState(currentGlobalFilter);
  const [prevAdvancedFilter, setPrevAdvancedFilter] = useState(
    currentAdvancedFilter,
  );

  const filtersChanged =
    prevFilters !== currentFilters || prevGlobalFilter !== currentGlobalFilter;
  const advancedFilterChanged = prevAdvancedFilter !== currentAdvancedFilter;

  if (filtersChanged || advancedFilterChanged) {
    setPrevFilters(currentFilters);
    setPrevGlobalFilter(currentGlobalFilter);
    setPrevAdvancedFilter(currentAdvancedFilter);

    if (
      enablePagination &&
      !manualPagination &&
      autoResetPageIndex == null &&
      (filtersChanged || enableAdvancedFilter)
    ) {
      table.setPageIndex(0);
    }
  }
}
