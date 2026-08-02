import type { Header, RowData } from "@tanstack/react-table";

import { Button } from "@/components/ui/button";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import { cn } from "@/lib/utils";

/** Drag handle to grab the resize edge of a column header. */
export function ColumnResizeHandle<TData extends RowData, TValue>({
  header,
  table,
}: {
  header: Header<TData, TValue>;
  table: DataTableInstance<TData>;
}) {
  if (!header.column.getCanResize()) return null;
  return (
    <Button
      aria-label={table.tableInstance.localization.resizeColumn}
      onMouseDown={header.getResizeHandler()}
      onTouchStart={header.getResizeHandler()}
      onDoubleClick={() =>
        table.tableInstance.enableColumnAutosize
          ? table.tableInstance.autoSizeColumn(header.column.id)
          : header.column.resetSize()
      }
      onContextMenu={() => header.column.resetSize()}
      className={cn(
        // Hidden at rest; a faint divider appears while the cursor is anywhere
        // in the header row (group/th), strengthens to primary on direct hover,
        // and stays primary while actively resizing.
        "absolute top-0 right-0 z-10 h-full px-0 w-1.5 cursor-col-resize hover:cursor-col-resize touch-none bg-transparent transition-colors select-none group-hover/th:bg-border/60 hover:bg-primary",
        // Touch devices can't hover, so the grip would be invisible: on a coarse
        // pointer reveal it at rest and widen the hit area for a finger.
        "pointer-coarse:w-1.5",
        header.column.getIsResizing()
          ? "bg-primary"
          : "pointer-coarse:bg-border/60",
      )}
    />
  );
}
