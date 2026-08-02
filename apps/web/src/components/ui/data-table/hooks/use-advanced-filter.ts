import type { RowData, RowModel, Table } from "@tanstack/react-table";
import React from "react";

import type { AdvancedFilterGroup } from "@/components/ui/data-table/core/types";
import { createAdvancedFilteredRowModel } from "@/components/ui/data-table/fns/advanced-filter";
import { useControllableState } from "@/components/ui/data-table/hooks/use-controllable-state";

const EMPTY_GROUP: AdvancedFilterGroup = { logic: "and", rules: [] };

interface UseAdvancedFilterParams {
  enableAdvancedFilter: boolean;
  advancedFilter?: AdvancedFilterGroup;
  defaultAdvancedFilter?: AdvancedFilterGroup;
  onAdvancedFilterChange?: (filter: AdvancedFilterGroup) => void;
}

export interface AdvancedFilterState<TData extends RowData> {
  advancedFilter: AdvancedFilterGroup;
  setAdvancedFilter: React.Dispatch<React.SetStateAction<AdvancedFilterGroup>>;
  showAdvancedFilterPanel: boolean;
  setShowAdvancedFilterPanel: React.Dispatch<React.SetStateAction<boolean>>;
  /** Filtered row model that applies the advanced group on top of the normal
   *  column/global filters. Stable identity (preserves TanStack memoization). */
  advancedFilteredRowModel: (table: Table<TData>) => () => RowModel<TData>;
}

/**
 * Advanced filter state (controllable group + panel visibility) plus the custom
 * filtered row model it drives. The active group is read through a ref so the
 * row-model factory keeps a stable identity; when the feature is off the ref
 * holds the empty group, so the model is a pass-through.
 */
export function useAdvancedFilter<TData extends RowData>({
  enableAdvancedFilter,
  advancedFilter: advancedFilterProp,
  defaultAdvancedFilter,
  onAdvancedFilterChange,
}: UseAdvancedFilterParams): AdvancedFilterState<TData> {
  const [advancedFilter, setAdvancedFilter] =
    useControllableState<AdvancedFilterGroup>(
      advancedFilterProp,
      defaultAdvancedFilter ?? EMPTY_GROUP,
      onAdvancedFilterChange,
    );
  const [showAdvancedFilterPanel, setShowAdvancedFilterPanel] =
    React.useState(false);

  const group = enableAdvancedFilter ? advancedFilter : EMPTY_GROUP;

  const advancedFilteredRowModel = React.useMemo(
    () => createAdvancedFilteredRowModel<TData>(() => group),
    [group],
  );

  return {
    advancedFilter,
    setAdvancedFilter,
    showAdvancedFilterPanel,
    setShowAdvancedFilterPanel,
    advancedFilteredRowModel,
  };
}
