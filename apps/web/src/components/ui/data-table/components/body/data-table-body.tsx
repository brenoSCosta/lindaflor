import {
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import type { Cell, Row, RowData } from "@tanstack/react-table";
import type { Virtualizer } from "@tanstack/react-virtual";
import React from "react";

import { DataTableBodyRow } from "@/components/ui/data-table/components/body/dnd/body-row";
import { renderBodyCell } from "@/components/ui/data-table/components/body/render-body-cell";
import { SkeletonRows } from "@/components/ui/data-table/components/body/skeleton-rows";
import { DataTableCreateRow } from "@/components/ui/data-table/components/editing/data-table-create-row";
import {
  ALIGN_CELL,
  DENSITY_CELL_PADDING,
  NON_DATA_COLUMN_IDS,
} from "@/components/ui/data-table/core/constants";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import type {
  VirtualRowItem,
  WithColumnSpacers,
} from "@/components/ui/data-table/hooks/use-table-virtualizers";
import {
  getColumnPinningClass,
  getColumnPinningStyle,
  getWidthStyle,
} from "@/components/ui/data-table/utils/column-styles";
import { isDataTableCellStriped } from "@/components/ui/data-table/utils/stripe-attrs";
import { TableBody, TableCell, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";

interface DataTableBodyProps<TData extends RowData> {
  table: DataTableInstance<TData>;
  rowVirtualizer: Virtualizer<HTMLDivElement, HTMLTableRowElement>;
  virtualItems: VirtualRowItem<TData>[];
  virtualColumns: { index: number }[];
  withColumnSpacers: WithColumnSpacers;
}

/**
 * The table `<tbody>`: the optional inline create-row, then either skeleton
 * rows (initial load), the virtualized window, the normal (sortable) rows, or
 * the empty-state row. Owns per-row / per-cell rendering, tree indentation, and
 * expanded detail panels.
 */
export function DataTableBody<TData extends RowData>({
  table,
  rowVirtualizer,
  virtualItems,
  virtualColumns,
  withColumnSpacers,
}: DataTableBodyProps<TData>) {
  const {
    density,
    enableKeyboardNavigation,
    enableFilterMatchHighlighting,
    columnsWithCustomCell,
    enableColumnResizing,
    enableColumnVirtualization,
    enableRowVirtualization,
    enableRowOrdering,
    renderDetailPanel,
    onRowClick,
    onRowDoubleClick,
    onCellClick,
    onCellDoubleClick,
    renderEmpty,
    enablePagination,
    showSkeletons,
    localization,
    striping,
    editDisplayMode,
    editingRowId,
  } = table.tableInstance;

  const padding = DENSITY_CELL_PADDING[density];
  const visibleColumnCount = table.getVisibleLeafColumns().length;
  const topRows = table.getTopRows();
  const bottomRows = table.getBottomRows();
  const centerRows = table.getCenterRows();
  const hasRows = topRows.length + centerRows.length + bottomRows.length > 0;
  const centerRowIds = centerRows.map((r) => r.id);
  const topCount = topRows.length;

  // Tree data (getSubRows) indents the first real data column by row depth so
  // the hierarchy is visible (grouped rows self-indent via the group cell).
  const isTreeData = table.options.getSubRows != null;
  const firstDataColumnId = table
    .getVisibleLeafColumns()
    .find((c) => !NON_DATA_COLUMN_IDS.has(c.id))?.id;

  const renderCell = (
    cell: Cell<TData, unknown>,
    row: Row<TData>,
    rowIndex: number,
    colIndex: number,
  ) => {
    const align = cell.column.columnDef.meta?.align ?? "left";
    const treeIndent =
      isTreeData &&
      cell.column.id === firstDataColumnId &&
      !cell.getIsGrouped() &&
      row.depth > 0
        ? row.depth
        : 0;
    const content = renderBodyCell(
      cell,
      table,
      enableFilterMatchHighlighting,
      columnsWithCustomCell,
      localization,
    );
    return (
      <TableCell
        key={cell.id}
        data-cell-row={rowIndex}
        data-cell-col={colIndex}
        data-pinned={cell.column.getIsPinned() || undefined}
        data-striped={
          isDataTableCellStriped(striping, rowIndex, colIndex, "body") ||
          undefined
        }
        tabIndex={
          enableKeyboardNavigation
            ? rowIndex === 0 && colIndex === 0
              ? 0
              : -1
            : undefined
        }
        style={{
          ...getWidthStyle(cell.column, table),
          ...getColumnPinningStyle(cell.column),
        }}
        onClick={
          onCellClick
            ? (event) => onCellClick({ cell, row, table, event })
            : undefined
        }
        onDoubleClick={
          onCellDoubleClick
            ? (event) => onCellDoubleClick({ cell, row, table, event })
            : undefined
        }
        className={cn(
          "relative bg-background",
          padding,
          ALIGN_CELL[align],
          // Fixed layout (resizing on) clips overflowing content with an
          // ellipsis instead of letting it bleed into the next column.
          enableColumnResizing && "overflow-hidden text-ellipsis",
          getColumnPinningClass(cell.column),
          enableKeyboardNavigation &&
            "focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:-outline-offset-2 focus-visible:outline-none",
        )}
      >
        {treeIndent > 0 ? (
          <span
            className="flex items-center"
            style={{ paddingInlineStart: `${treeIndent}rem` }}
          >
            {content}
          </span>
        ) : (
          content
        )}
      </TableCell>
    );
  };

  const renderCells = (row: Row<TData>, rowIndex: number): React.ReactNode => {
    const cells = row.getVisibleCells();
    if (!enableColumnVirtualization) {
      return cells.map((cell, colIndex) =>
        renderCell(cell, row, rowIndex, colIndex),
      );
    }
    return withColumnSpacers(
      virtualColumns.flatMap((vc) => {
        const cell = cells[vc.index];
        return cell ? renderCell(cell, row, rowIndex, vc.index) : null;
      }) as React.ReactNode[],
      `row-${row.id}`,
    );
  };

  const detailRow = (row: Row<TData>) => (
    <TableRow className="hover:bg-transparent">
      <TableCell colSpan={visibleColumnCount} className="bg-muted/20 p-0">
        <div className="p-3">{renderDetailPanel?.({ row, table })}</div>
      </TableCell>
    </TableRow>
  );

  const renderRow = (row: Row<TData>, rowIndex: number) => {
    const isGrouped = row.getIsGrouped();
    const showDetail = !!renderDetailPanel && row.getIsExpanded() && !isGrouped;
    const cells = renderCells(row, rowIndex);
    const isRowBeingEdited =
      editDisplayMode === "row" && editingRowId === row.id;

    return (
      <React.Fragment key={row.id}>
        <DataTableBodyRow
          row={row}
          draggable={
            enableRowOrdering &&
            !enableRowVirtualization &&
            !row.getIsPinned() &&
            !isGrouped
          }
          className={isRowBeingEdited ? "bg-muted/20" : undefined}
          onClick={
            onRowClick
              ? (event) => onRowClick({ row, table, event })
              : undefined
          }
          onDoubleClick={
            onRowDoubleClick
              ? (event) => onRowDoubleClick({ row, table, event })
              : undefined
          }
        >
          {cells}
        </DataTableBodyRow>
        {showDetail && detailRow(row)}
      </React.Fragment>
    );
  };

  return (
    <TableBody>
      {table.tableInstance.enableEditing &&
        table.tableInstance.isCreating &&
        table.tableInstance.createDisplayMode === "row" && (
          <DataTableCreateRow table={table} />
        )}
      {showSkeletons && !hasRows ? (
        <SkeletonRows
          rowCount={enablePagination ? table.getState().pagination.pageSize : 8}
          columnCount={visibleColumnCount}
          padding={padding}
        />
      ) : hasRows && enableRowVirtualization ? (
        <>
          {topRows.map((row, i) => renderRow(row, i))}
          {(() => {
            const vRows = rowVirtualizer.getVirtualItems();
            const padTop = vRows.length ? (vRows[0]?.start ?? 0) : 0;
            const padBottom = vRows.length
              ? rowVirtualizer.getTotalSize() -
                (vRows[vRows.length - 1]?.end ?? 0)
              : 0;
            return (
              <>
                {padTop > 0 && (
                  // oxlint-disable-next-line react-doctor/no-aria-hidden-on-focusable - the row is only structural layout for virtualization and contains no real data.
                  <tr aria-hidden>
                    {/* oxlint-disable-next-line react-doctor/control-has-associated-label */}
                    <td
                      colSpan={visibleColumnCount}
                      style={{ height: padTop, padding: 0, border: 0 }}
                    />
                  </tr>
                )}
                {vRows.map((vRow) => {
                  const item = virtualItems[vRow.index];
                  if (!item) return null;
                  if (item.detail) {
                    return (
                      <TableRow
                        key={`${item.row.id}-detail`}
                        data-index={vRow.index}
                        ref={rowVirtualizer.measureElement}
                        className="hover:bg-transparent"
                      >
                        <TableCell
                          colSpan={visibleColumnCount}
                          className="bg-muted/20 p-0"
                        >
                          <div className="p-3">
                            {renderDetailPanel?.({ row: item.row, table })}
                          </div>
                        </TableCell>
                      </TableRow>
                    );
                  }
                  const cells = renderCells(item.row, vRow.index);
                  return (
                    <TableRow
                      key={item.row.id}
                      data-index={vRow.index}
                      ref={rowVirtualizer.measureElement}
                      data-state={
                        item.row.getIsSelected() ? "selected" : undefined
                      }
                      onClick={
                        onRowClick
                          ? (event) =>
                              onRowClick({ row: item.row, table, event })
                          : undefined
                      }
                      onDoubleClick={
                        onRowDoubleClick
                          ? (event) =>
                              onRowDoubleClick({ row: item.row, table, event })
                          : undefined
                      }
                      className={cn(
                        "data-[state=selected]:shadow-[inset_2px_0_0_0_var(--primary)]",
                        (onRowClick || onRowDoubleClick) && "cursor-pointer",
                        editDisplayMode === "row" &&
                          editingRowId === item.row.id &&
                          "bg-muted/20",
                      )}
                    >
                      {cells}
                    </TableRow>
                  );
                })}
                {padBottom > 0 && (
                  // oxlint-disable-next-line react-doctor/no-aria-hidden-on-focusable - the row is only structural layout for virtualization and contains no real data.
                  <tr aria-hidden>
                    {/* oxlint-disable-next-line react-doctor/control-has-associated-label */}
                    <td
                      colSpan={visibleColumnCount}
                      style={{ height: padBottom, padding: 0, border: 0 }}
                    />
                  </tr>
                )}
              </>
            );
          })()}
          {bottomRows.map((row, i) => renderRow(row, topCount + i))}
        </>
      ) : hasRows ? (
        <>
          {topRows.map((row, i) => renderRow(row, i))}
          <SortableContext
            items={centerRowIds}
            strategy={verticalListSortingStrategy}
          >
            {centerRows.map((row, i) => renderRow(row, topRows.length + i))}
          </SortableContext>
          {bottomRows.map((row, i) =>
            renderRow(row, topRows.length + centerRows.length + i),
          )}
        </>
      ) : (
        <TableRow className="hover:bg-transparent">
          <TableCell
            colSpan={visibleColumnCount}
            className="h-32 text-center text-sm text-muted-foreground"
          >
            {renderEmpty?.({ table }) ?? localization.noRecordsToDisplay}
          </TableCell>
        </TableRow>
      )}
    </TableBody>
  );
}
