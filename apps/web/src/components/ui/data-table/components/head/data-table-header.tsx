import {
  SortableContext,
  horizontalListSortingStrategy,
} from "@dnd-kit/sortable";
import type { Header, RowData } from "@tanstack/react-table";
import type { VirtualItem } from "@tanstack/react-virtual";
import React from "react";

import { DataTableHeadCell } from "@/components/ui/data-table/components/body/dnd/head-cell";
import { DataTableColumnFilter } from "@/components/ui/data-table/components/head/data-table-column-filter";
import { DataTableColumnHeader } from "@/components/ui/data-table/components/head/data-table-column-header";
import { DENSITY_CELL_PADDING } from "@/components/ui/data-table/core/constants";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import {
  headerControlsOptionsFromTable,
  shouldShowColumnDragGrip,
} from "@/components/ui/data-table/helpers/header-controls";
import type { WithColumnSpacers } from "@/components/ui/data-table/hooks/use-table-virtualizers";
import {
  getColumnPinningClass,
  getColumnPinningStyle,
  getWidthStyle,
} from "@/components/ui/data-table/utils/column-styles";
import { isDataTableCellStriped } from "@/components/ui/data-table/utils/stripe-attrs";
import { TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";

interface DataTableHeaderProps<TData extends RowData> {
  table: DataTableInstance<TData>;
  virtualColumns: VirtualItem[];
  withColumnSpacers: WithColumnSpacers;
}

/**
 * The table `<thead>`: the (optionally sticky) header-group rows and, when
 * enabled, the per-column filter subheader row. Honors column virtualization,
 * ordering (drag handles), pinning, and resizing.
 */
export function DataTableHeader<TData extends RowData>({
  table,
  virtualColumns,
  withColumnSpacers,
}: DataTableHeaderProps<TData>) {
  const {
    density,
    enableColumnResizing,
    enableColumnVirtualization,
    enableColumnFilters,
    showColumnFilters,
    columnFilterDisplayMode,
    enableStickyHeader,
    striping,
    refs: tableRefs,
  } = table.tableInstance;

  const controls = headerControlsOptionsFromTable(table);
  const padding = DENSITY_CELL_PADDING[density];
  const leafColumnIds = table.getVisibleLeafColumns().map((c) => c.id);

  const anyFilterable = table
    .getAllColumns()
    .some((column) => column.getCanFilter());
  const filterRowVisible =
    enableColumnFilters &&
    showColumnFilters &&
    anyFilterable &&
    columnFilterDisplayMode === "subheader";

  const renderHeadCell = (header: Header<TData, unknown>, colIndex: number) => (
    <DataTableHeadCell
      key={header.id}
      header={header}
      table={table}
      // Draggable for reordering (column ordering) or to drag onto the group
      // zone (grouping) — the drag-end handler routes by the drop target. The
      // grip's presence drives the column's reserved width, so both read the
      // same predicate (see helpers/header-controls).
      draggable={shouldShowColumnDragGrip(header.column, controls)}
      resizable={enableColumnResizing}
      widthStyle={getWidthStyle(header.column, table)}
      padding={padding}
      colIndex={colIndex}
    >
      {header.isPlaceholder ? null : (
        <DataTableColumnHeader header={header} table={table} />
      )}
    </DataTableHeadCell>
  );

  const renderFilterCell = (
    header: Header<TData, unknown>,
    colIndex: number,
  ) => (
    <TableHead
      key={header.id}
      colSpan={header.colSpan}
      style={{
        ...getWidthStyle(header.column, table),
        ...getColumnPinningStyle(header.column),
      }}
      data-striped={
        isDataTableCellStriped(striping, 0, colIndex, "head") || undefined
      }
      className={cn("bg-background", getColumnPinningClass(header.column))}
    >
      <DataTableColumnFilter header={header} table={table} />
    </TableHead>
  );

  const tableHeadRef = tableRefs?.tableHeadRef;

  return (
    <TableHeader
      ref={tableHeadRef}
      className={cn(enableStickyHeader && "sticky top-0 z-20 bg-background")}
    >
      {table.getHeaderGroups().map((headerGroup) => (
        <TableRow
          key={headerGroup.id}
          className="group/th hover:bg-transparent"
        >
          {enableColumnVirtualization ? (
            withColumnSpacers(
              virtualColumns.flatMap((vc) => {
                const header = headerGroup.headers[vc.index];
                return header ? renderHeadCell(header, vc.index) : null;
              }) as React.ReactNode[],
              `head-${headerGroup.id}`,
            )
          ) : (
            <SortableContext
              items={leafColumnIds}
              strategy={horizontalListSortingStrategy}
            >
              {headerGroup.headers.map((header, colIndex) =>
                renderHeadCell(header, colIndex),
              )}
            </SortableContext>
          )}
        </TableRow>
      ))}

      {filterRowVisible &&
        table.getHeaderGroups().map((headerGroup) => (
          <TableRow
            key={`${headerGroup.id}-filters`}
            className="hover:bg-transparent"
          >
            {enableColumnVirtualization
              ? withColumnSpacers(
                  virtualColumns.flatMap((vc) => {
                    const header = headerGroup.headers[vc.index];
                    return header ? renderFilterCell(header, vc.index) : null;
                  }) as React.ReactNode[],
                  `filter-${headerGroup.id}`,
                )
              : headerGroup.headers.map((header, colIndex) =>
                  renderFilterCell(header, colIndex),
                )}
          </TableRow>
        ))}
    </TableHeader>
  );
}
