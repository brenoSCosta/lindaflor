import type { FilterFn, RowData, RowModel, Table } from "@tanstack/react-table";
import React from "react";

import { createGlobalFilterFn } from "@/components/ui/data-table/fns/filter-factories";
import type { GlobalFilterMode } from "@/components/ui/data-table/fns/filter-modes";
import { createRankedSortedRowModel } from "@/components/ui/data-table/fns/ranked-row-model";
import { useControllableState } from "@/components/ui/data-table/hooks/use-controllable-state";

interface UseGlobalFilterModeParams {
  globalFilterMode?: GlobalFilterMode;
  defaultGlobalFilterMode: GlobalFilterMode;
  onGlobalFilterModeChange?: (mode: GlobalFilterMode) => void;
  enableGlobalFilterRankedResults: boolean;
  manualSorting: boolean;
  manualFiltering: boolean;
  enableGrouping: boolean;
}

export interface GlobalFilterModeState<TData extends RowData> {
  globalFilterMode: GlobalFilterMode;
  setGlobalFilterMode: (mode: GlobalFilterMode) => void;
  /** Mode-aware global filter fn (new identity per mode → re-runs filtering). */
  dynamicGlobalFilterFn: FilterFn<TData>;
  /** Sorted row model that re-orders by fuzzy rank when ranking is active. */
  rankedSortedRowModel: (table: Table<TData>) => () => RowModel<TData>;
}

/**
 * Global-search mode state plus the two row-model pieces it drives: the
 * mode-aware global filter fn and the fuzzy-ranked sorted row model. While a
 * fuzzy global search is active and the user hasn't sorted, rows are ordered by
 * best match; the current config is read through a ref so the model factory
 * keeps a stable identity (recreating it would defeat TanStack's memoization).
 */
export function useGlobalFilterMode<TData extends RowData>({
  globalFilterMode: globalFilterModeProp,
  defaultGlobalFilterMode,
  onGlobalFilterModeChange,
  enableGlobalFilterRankedResults,
  manualSorting,
  manualFiltering,
  enableGrouping,
}: UseGlobalFilterModeParams): GlobalFilterModeState<TData> {
  const [globalFilterMode, setGlobalFilterMode] =
    useControllableState<GlobalFilterMode>(
      globalFilterModeProp,
      defaultGlobalFilterMode,
      onGlobalFilterModeChange,
    );

  // Recreating the fn when the mode changes gives it a new identity, which
  // makes TanStack re-run global filtering with the new mode immediately.
  const dynamicGlobalFilterFn = React.useMemo(
    () => createGlobalFilterFn<TData>(() => globalFilterMode),
    [globalFilterMode],
  );

  const rankedSortedRowModel = React.useMemo(
    () =>
      createRankedSortedRowModel<TData>((t) => {
        if (!enableGlobalFilterRankedResults || globalFilterMode !== "fuzzy")
          return false;
        if (manualSorting || manualFiltering) return false;
        const s = t.getState();
        if (!s.globalFilter) return false;
        if (s.sorting.some(Boolean)) return false;
        if (enableGrouping && s.grouping.length > 0) return false;
        if (s.expanded === true || Object.values(s.expanded).some(Boolean))
          return false;
        return true;
      }),
    [
      enableGlobalFilterRankedResults,
      globalFilterMode,
      manualSorting,
      manualFiltering,
      enableGrouping,
    ],
  );

  return {
    globalFilterMode,
    setGlobalFilterMode,
    dynamicGlobalFilterFn,
    rankedSortedRowModel,
  };
}
