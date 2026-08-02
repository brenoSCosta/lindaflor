import type { Cell, Row, RowData } from "@tanstack/react-table";
import React from "react";

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { ClickToCopy } from "@/components/ui/data-table/components/body/click-to-copy";
import { DataTableEditField } from "@/components/ui/data-table/components/editing/data-table-edit-field";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import { getColumnLabel } from "@/components/ui/data-table/helpers/column-label";
import { isColumnEditable } from "@/components/ui/data-table/helpers/is-column-editable";

/**
 * Resolves a leaf data cell's interactive content: an inline editor when this
 * cell/row is being edited (cell/row/table modes), otherwise the value wrapped
 * with click-to-copy, click-to-edit (cell mode), and/or a cell-actions context
 * menu as configured. `fallback` is the normal rendered value.
 */
export function DataTableBodyCellContent<TData extends RowData>({
  cell,
  table,
  fallback,
}: {
  cell: Cell<TData, unknown>;
  table: DataTableInstance<TData>;
  fallback: React.ReactNode;
}) {
  const cn = table.tableInstance;
  const { row, column } = cell;
  const editable = cn.enableEditing && isColumnEditable(column);
  const mode = cn.editDisplayMode;

  const isCellEditing =
    mode === "cell" &&
    cn.editingCell?.rowId === row.id &&
    cn.editingCell?.columnId === column.id;
  const isRowEditing = mode === "row" && cn.editingRowId === row.id;
  const isTableEditing = mode === "table";

  if (editable && (isCellEditing || isRowEditing || isTableEditing)) {
    const renderEditCell = column.columnDef.meta?.renderEditCell;
    if (renderEditCell) {
      return renderEditCell({ cell, row, column, table });
    }
    return mode === "row" ? (
      <RowDraftEditor cell={cell} table={table} />
    ) : (
      <LocalDraftEditor
        cell={cell}
        table={table}
        exitOnCommit={mode === "cell"}
      />
    );
  }

  let node: React.ReactNode = fallback;

  const copyEnabled =
    column.columnDef.meta?.enableClickToCopy ?? cn.enableClickToCopy;
  if (copyEnabled) {
    node = (
      <ClickToCopy
        value={cell.getValue.toString() ?? ""}
        copyLabel={cn.localization.copy}
        copiedLabel={cn.localization.copied}
      >
        {node}
      </ClickToCopy>
    );
  } else if (editable && mode === "cell") {
    node = (
      <button
        type="button"
        onClick={() =>
          cn.setEditingCell({ rowId: row.id, columnId: column.id })
        }
        className="-mx-1 w-full rounded-sm px-1 text-left transition-colors outline-none hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring/40"
      >
        {node}
      </button>
    );
  }

  if (cn.renderCellActionMenuItems) {
    node = (
      <ContextMenu>
        <ContextMenuTrigger render={<span className="block" />}>
          {node}
        </ContextMenuTrigger>
        <ContextMenuContent>
          <CellActionMenuItems cell={cell} row={row} table={table} />
        </ContextMenuContent>
      </ContextMenu>
    );
  }

  return node;
}

function CellActionMenuItems<TData extends RowData>({
  cell,
  row,
  table,
}: {
  cell: Cell<TData, unknown>;
  row: Row<TData>;
  table: DataTableInstance<TData>;
}) {
  const { renderCellActionMenuItems } = table.tableInstance;
  if (!renderCellActionMenuItems) return null;
  const Component = renderCellActionMenuItems;
  return <Component cell={cell} row={row} table={table} />;
}

/** Editor bound to the shared row draft (row/modal editing). */
function RowDraftEditor<TData extends RowData>({
  cell,
  table,
}: {
  cell: Cell<TData, unknown>;
  table: DataTableInstance<TData>;
}) {
  const cn = table.tableInstance;
  const { column } = cell;
  const meta = column.columnDef.meta;
  const value =
    column.id in cn.rowDraft ? cn.rowDraft[column.id] : cell.getValue();
  const error = meta?.validate?.(value);
  return (
    <DataTableEditField
      value={value}
      variant={meta?.editVariant}
      options={meta?.editSelectOptions ?? meta?.options}
      error={error}
      ariaLabel={getColumnLabel(column)}
      onChange={(next) => cn.setRowDraftValue(column.id, next)}
    />
  );
}

/** Editor with local draft state (cell + table editing). */
function LocalDraftEditor<TData extends RowData>({
  cell,
  table,
  exitOnCommit,
}: {
  cell: Cell<TData, unknown>;
  table: DataTableInstance<TData>;
  exitOnCommit: boolean;
}) {
  const cn = table.tableInstance;
  const { row, column } = cell;
  const meta = column.columnDef.meta;
  const [draft, setDraft] = React.useState<unknown>(() => cell.getValue());
  const error = meta?.validate?.(draft);

  const commit = () => {
    if (error) return;
    if (draft !== cell.getValue()) {
      cn.onEditCellSave?.({ row, column, value: draft, table });
    }
    if (exitOnCommit) cn.setEditingCell(null);
  };

  const cancel = () => {
    setDraft(cell.getValue());
    if (exitOnCommit) cn.setEditingCell(null);
  };

  return (
    <DataTableEditField
      value={draft}
      variant={meta?.editVariant}
      options={meta?.editSelectOptions ?? meta?.options}
      error={error}
      ariaLabel={getColumnLabel(column)}
      autoFocus={exitOnCommit}
      onChange={setDraft}
      onCommit={commit}
      onCancel={cancel}
    />
  );
}
