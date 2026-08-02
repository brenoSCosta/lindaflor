import type { Density } from "@/components/ui/data-table/core/types";
import { ROW_ACTIONS_COLUMN_ID } from "@/components/ui/data-table/injected-columns/data-table-row-actions";
import { EXPAND_COLUMN_ID } from "@/components/ui/data-table/injected-columns/expand-column";
import { ROW_DRAG_COLUMN_ID } from "@/components/ui/data-table/injected-columns/row-drag-column";
import { ROW_NUMBER_COLUMN_ID } from "@/components/ui/data-table/injected-columns/row-number-column";
import { SELECTION_COLUMN_ID } from "@/components/ui/data-table/injected-columns/selection-column";

export const DENSITY_ORDER: Density[] = ["comfortable", "compact", "spacious"];

/** Vertical padding utility per density level, applied to header + body cells. */
export const DENSITY_CELL_PADDING: Record<Density, string> = {
  compact: "py-1",
  comfortable: "py-2.5",
  spacious: "py-4",
};

/** Descendant selectors for cells marked with `data-striped`. */
export const DATA_TABLE_STRIPING_CLASS =
  "[&_[data-slot=table-head][data-striped]]:bg-muted [&_[data-slot=table-cell][data-striped]]:bg-muted";

/** Horizontal alignment → text-align utility, applied to body cells. */
export const ALIGN_CELL = {
  left: "text-left",
  center: "text-center",
  right: "text-right",
} as const;

/** Injected columns that should never be draggable in the header. */
export const DISPLAY_COLUMN_IDS = new Set([
  SELECTION_COLUMN_ID,
  ROW_NUMBER_COLUMN_ID,
  ROW_DRAG_COLUMN_ID,
]);

// All injected (non-user) columns, used to find the first real data column so
// tree (sub-row) rows can be indented by depth there.
export const NON_DATA_COLUMN_IDS = new Set([
  SELECTION_COLUMN_ID,
  ROW_NUMBER_COLUMN_ID,
  ROW_DRAG_COLUMN_ID,
  EXPAND_COLUMN_ID,
  ROW_ACTIONS_COLUMN_ID,
]);
