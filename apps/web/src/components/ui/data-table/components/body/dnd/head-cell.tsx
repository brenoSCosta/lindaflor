import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import type { Header, RowData } from "@tanstack/react-table";
import React from "react";

import { Button } from "@/components/ui/button";
import { ColumnResizeHandle } from "@/components/ui/data-table/components/body/dnd/column-resize-handle";
import type { IconComponent } from "@/components/ui/data-table/core/icons";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import {
  getColumnPinningClass,
  getColumnPinningStyle,
} from "@/components/ui/data-table/utils/column-styles";
import { isDataTableCellStriped } from "@/components/ui/data-table/utils/stripe-attrs";
import { TableHead } from "@/components/ui/table";
import { cn } from "@/lib/utils";

/** dnd-kit activator props for the current column's drag handle. */
export interface ColumnDragHandleProps {
  attributes: Record<string, unknown>;
  listeners: Record<string, unknown> | undefined;
  setActivatorNodeRef: (el: HTMLElement | null) => void;
}

/** Provided by the sortable header cell, consumed by {@link ColumnDragHandle}
 *  so the grip can live inside the column header (beside the actions menu)
 *  while the dnd-kit wiring stays on the cell. Null when the column isn't
 *  draggable. */
export const ColumnDragContext =
  React.createContext<ColumnDragHandleProps | null>(null);

/**
 * Header cell. `useSortable` is always called (disabled when ordering is off or
 * for display columns) to keep hook order stable. Applies pinning + width
 * styles, an optional drag grip, and the resize handle.
 */
export function DataTableHeadCell<TData extends RowData, TValue>({
  header,
  table,
  draggable,
  resizable,
  widthStyle,
  padding,
  colIndex,
  children,
}: {
  header: Header<TData, TValue>;
  table: DataTableInstance<TData>;
  draggable: boolean;
  resizable: boolean;
  widthStyle: React.CSSProperties;
  padding: string;
  colIndex: number;
  children: React.ReactNode;
}) {
  const column = header.column;
  const { striping } = table.tableInstance;
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    isDragging,
  } = useSortable({
    id: column.id,
    disabled: !draggable,
    data: { type: "column" },
  });

  const style: React.CSSProperties = {
    ...widthStyle,
    ...getColumnPinningStyle(column),
    transform: CSS.Translate.toString(transform),
    // Animate width for programmatic changes (autosize, pinning) but not during
    // an active drag, where the easing makes the header lag behind the cursor.
    transition: column.getIsResizing() ? undefined : "width 0.15s ease",
    opacity: isDragging ? 0.7 : undefined,
    zIndex: isDragging ? 30 : undefined,
  };

  // Hand the drag-activator props to the header (via context) so the grip can
  // render next to the column-actions menu instead of crowding the left edge.
  const dragProps = React.useMemo<ColumnDragHandleProps>(
    () => ({
      attributes: Object.fromEntries(Object.entries(attributes)),
      listeners: listeners
        ? Object.fromEntries(Object.entries(listeners))
        : undefined,
      setActivatorNodeRef,
    }),
    [attributes, listeners, setActivatorNodeRef],
  );

  return (
    <TableHead
      ref={setNodeRef}
      colSpan={header.colSpan}
      style={style}
      data-pinned={column.getIsPinned() || undefined}
      data-striped={
        isDataTableCellStriped(striping, 0, colIndex, "head") || undefined
      }
      aria-sort={ariaSort(column.getIsSorted())}
      className={cn(
        "relative bg-background",
        padding,
        // Match the body: under fixed layout, keep long header labels from
        // bleeding past the (resizable) column edge.
        resizable && "overflow-hidden",
        getColumnPinningClass(column),
      )}
    >
      <ColumnDragContext.Provider value={draggable ? dragProps : null}>
        {children}
      </ColumnDragContext.Provider>
      {resizable && <ColumnResizeHandle header={header} table={table} />}
    </TableHead>
  );
}

/** Column reorder grip. Reads dnd-kit props from {@link ColumnDragContext} and
 *  renders nothing when the column isn't draggable. Lives inside the column
 *  header, just before the column-actions menu. */
export function ColumnDragHandle({
  label,
  Icon,
}: {
  label: string;
  Icon: IconComponent;
}) {
  const ctx = React.use(ColumnDragContext);
  if (!ctx) return null;
  const { attributes, listeners, setActivatorNodeRef } = ctx;
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label={label}
      ref={setActivatorNodeRef}
      // dnd-kit injects an aria-describedby id that can differ between the
      // server and client render; suppress that benign attribute mismatch.
      suppressHydrationWarning
      {...attributes}
      {...listeners}
      // touch-none: let dnd-kit's pointer sensor own the touch gesture
      // (otherwise the browser scrolls/selects text before a drag can start on
      // a phone). size-7 matches the actions menu for a finger-friendly target.
      className="size-7 shrink-0 cursor-grab touch-none text-muted-foreground opacity-70 transition-opacity group-hover/th:opacity-100 focus-visible:opacity-100 active:cursor-grabbing"
    >
      <Icon className="size-3.5" />
    </Button>
  );
}

function ariaSort(
  sorted: false | "asc" | "desc",
): React.AriaAttributes["aria-sort"] {
  if (sorted === "asc") return "ascending";
  if (sorted === "desc") return "descending";
  return "none";
}
