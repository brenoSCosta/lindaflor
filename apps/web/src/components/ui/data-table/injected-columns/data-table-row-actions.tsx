import type { ColumnDef, Row, RowData, Table } from "@tanstack/react-table";

import { Button } from "@/components/ui/button";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";

export const ROW_ACTIONS_COLUMN_ID = "cn-row-actions";

function isDataTableInstance<TData extends RowData>(
  table: Table<TData>,
): table is DataTableInstance<TData> {
  return "tableInstance" in table;
}

/** Actions column: edit/save/cancel controls + the consumer's
 *  `renderRowActions` slot. When positioned `"first"` the controls align to the
 *  left edge; when `"last"` (default) they align to the right. */
export function createRowActionsColumn<TData extends RowData>(
  position: "first" | "last" = "last",
): ColumnDef<TData> {
  const align = position === "first" ? "left" : "right";
  return {
    id: ROW_ACTIONS_COLUMN_ID,
    enableSorting: false,
    enableHiding: false,
    enableColumnFilter: false,
    enableResizing: false,
    enableGrouping: false,
    size: 44,
    minSize: 44,
    meta: { disableColumnActions: true, align },
    header: () => null,
    cell: ({ row, table }) => {
      if (!isDataTableInstance(table)) return null;
      return <RowActionsCell row={row} table={table} align={align} />;
    },
  };
}

function RowActionMenuItems<TData extends RowData>({
  row,
  table,
}: {
  row: Row<TData>;
  table: DataTableInstance<TData>;
}) {
  const { renderRowActionMenuItems } = table.tableInstance;
  if (!renderRowActionMenuItems) return null;
  const Component = renderRowActionMenuItems;
  return <Component row={row} table={table} />;
}

function RowActionsCell<TData extends RowData>({
  row,
  table,
  align,
}: {
  row: Row<TData>;
  table: DataTableInstance<TData>;
  align: "left" | "right";
}) {
  const {
    localization,
    icons,
    enableEditing,
    editDisplayMode,
    editingRowId,
    rowDraft,
    onSaveRow,
    beginRowEdit,
    cancelEdit,
    renderRowActions,
    renderRowActionMenuItems,
  } = table.tableInstance;

  const isEditingThisRow = editDisplayMode === "row" && editingRowId === row.id;

  if (isEditingThisRow) {
    return (
      <div
        className={cn(
          "flex items-center gap-1",
          align === "left" ? "justify-start" : "justify-end",
        )}
      >
        <Button
          variant="ghost"
          size="icon"
          aria-label={localization.save}
          className="size-7"
          onClick={() =>
            onSaveRow?.({ row, values: rowDraft, table, exit: cancelEdit })
          }
        >
          <icons.save />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          aria-label={localization.cancel}
          className="size-7"
          onClick={cancelEdit}
        >
          <icons.cancel />
        </Button>
      </div>
    );
  }

  const canInlineEdit =
    enableEditing && (editDisplayMode === "row" || editDisplayMode === "modal");

  return (
    <div
      className={cn(
        "flex items-center gap-1",
        align === "left" ? "justify-start" : "justify-end",
      )}
    >
      {canInlineEdit && (
        <Button
          variant="ghost"
          size="icon"
          aria-label={localization.edit}
          className="size-7"
          onClick={() => beginRowEdit(row)}
        >
          <icons.edit />
        </Button>
      )}
      {renderRowActions?.({ row, table })}
      {renderRowActionMenuItems && (
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="icon"
                aria-label={localization.rowActions}
                className="size-7"
              />
            }
          >
            <icons.columnActions />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <RowActionMenuItems row={row} table={table} />
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
