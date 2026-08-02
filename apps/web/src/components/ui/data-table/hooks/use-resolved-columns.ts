import type { ColumnDef, RowData } from "@tanstack/react-table";
import React from "react";

import type { DataTableIcons } from "@/components/ui/data-table/core/icons";
import type { DataTableLocalization } from "@/components/ui/data-table/core/localization";
import type {
  EditDisplayMode,
  UseDataTableOptions,
} from "@/components/ui/data-table/core/types";
import { createRowActionsColumn } from "@/components/ui/data-table/injected-columns/data-table-row-actions";
import { createExpandColumn } from "@/components/ui/data-table/injected-columns/expand-column";
import { createRowDragHandleColumn } from "@/components/ui/data-table/injected-columns/row-drag-column";
import { createRowNumberColumn } from "@/components/ui/data-table/injected-columns/row-number-column";
import { createSelectionColumn } from "@/components/ui/data-table/injected-columns/selection-column";

interface UseResolvedColumnsParams<TData extends RowData> {
  columns: ColumnDef<TData>[];
  enableRowOrdering: boolean;
  enableRowSelection: boolean;
  selectAllMode: "page" | "all";
  enableSelectAll: boolean;
  needsExpandColumn: boolean;
  positionExpandColumn: "first" | "last";
  enableRowNumbers: boolean;
  rowNumberMode: "static" | "original";
  enableRowPinning: boolean;
  renderRowActions: UseDataTableOptions<TData>["renderRowActions"];
  renderRowActionMenuItems: UseDataTableOptions<TData>["renderRowActionMenuItems"];
  positionActionsColumn: "first" | "last";
  enableEditing: boolean;
  editDisplayMode: EditDisplayMode;
  localization: DataTableLocalization;
  icons: DataTableIcons;
}

/**
 * Injects the display columns around the user's columns, memoized so TanStack
 * never receives a new `columns` identity per render (a classic infinite-loop /
 * lost-state trap). Leading order: drag handle → selection → expand → row
 * number → user columns. The expand and row-actions columns can move to the
 * trailing/leading edge via `positionExpandColumn` / `positionActionsColumn`.
 */
export function useResolvedColumns<TData extends RowData>({
  columns,
  enableRowOrdering,
  enableRowSelection,
  selectAllMode,
  enableSelectAll,
  needsExpandColumn,
  positionExpandColumn,
  enableRowNumbers,
  rowNumberMode,
  enableRowPinning,
  renderRowActions,
  renderRowActionMenuItems,
  positionActionsColumn,
  enableEditing,
  editDisplayMode,
  localization,
  icons,
}: UseResolvedColumnsParams<TData>): ColumnDef<TData>[] {
  return React.useMemo(() => {
    const leading = [];
    const trailing = [];
    if (enableRowOrdering) {
      leading.push(createRowDragHandleColumn<TData>(localization, icons));
    }
    if (enableRowSelection) {
      leading.push(
        createSelectionColumn<TData>(
          localization,
          selectAllMode,
          enableSelectAll,
        ),
      );
    }
    if (needsExpandColumn) {
      const expand = createExpandColumn<TData>(localization, icons);
      if (positionExpandColumn === "last") trailing.push(expand);
      else leading.push(expand);
    }
    if (enableRowNumbers) {
      leading.push(
        createRowNumberColumn<TData>(
          localization,
          rowNumberMode,
          enableRowPinning,
          icons,
        ),
      );
    }
    const showRowActions =
      !!renderRowActions ||
      !!renderRowActionMenuItems ||
      (enableEditing &&
        (editDisplayMode === "row" || editDisplayMode === "modal"));
    if (showRowActions) {
      const actions = createRowActionsColumn<TData>(positionActionsColumn);
      if (positionActionsColumn === "first") leading.unshift(actions);
      else trailing.push(actions);
    }
    return leading.length > 0 || trailing.length > 0
      ? [...leading, ...columns, ...trailing]
      : columns;
  }, [
    columns,
    enableRowOrdering,
    enableRowSelection,
    selectAllMode,
    enableSelectAll,
    needsExpandColumn,
    positionExpandColumn,
    enableRowNumbers,
    rowNumberMode,
    enableRowPinning,
    renderRowActions,
    renderRowActionMenuItems,
    positionActionsColumn,
    enableEditing,
    editDisplayMode,
    localization,
    icons,
  ]);
}
