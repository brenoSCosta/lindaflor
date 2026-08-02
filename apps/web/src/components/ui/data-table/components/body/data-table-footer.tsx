import { flexRender, type Header, type RowData } from "@tanstack/react-table";
import type { VirtualItem } from "@tanstack/react-virtual";

import { DENSITY_CELL_PADDING } from "@/components/ui/data-table/core/constants";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import type { WithColumnSpacers } from "@/components/ui/data-table/hooks/use-table-virtualizers";
import {
  getColumnPinningClass,
  getColumnPinningStyle,
  getWidthStyle,
} from "@/components/ui/data-table/utils/column-styles";
import { isDataTableCellStriped } from "@/components/ui/data-table/utils/stripe-attrs";
import { TableCell, TableFooter, TableRow } from "@/components/ui/table";
import { cn } from "@/lib/utils";

interface DataTableFooterProps<TData extends RowData> {
  table: DataTableInstance<TData>;
  virtualColumns: VirtualItem[];
  withColumnSpacers: WithColumnSpacers;
}

/** Whether any leaf column defines a footer (controls whether a `<tfoot>` renders). */
export function hasFooter<TData extends RowData>(
  table: DataTableInstance<TData>,
): boolean {
  return table.getAllLeafColumns().some((c) => c.columnDef.footer != null);
}

/**
 * The table `<tfoot>` (aggregation / footer cells). Stickiness is controlled by
 * `enableStickyFooter` (on by default), independent of whether a footer exists.
 */
export function DataTableFooter<TData extends RowData>({
  table,
  virtualColumns,
  withColumnSpacers,
}: DataTableFooterProps<TData>) {
  const {
    density,
    enableColumnVirtualization,
    enableStickyFooter,
    striping,
    refs: tableRefs,
  } = table.tableInstance;
  const padding = DENSITY_CELL_PADDING[density];

  const tableFooterRef = tableRefs?.tableFooterRef;
  return (
    <TableFooter
      ref={tableFooterRef}
      className={cn(enableStickyFooter && "sticky bottom-0 z-20")}
    >
      {table.getFooterGroups().map((footerGroup) => {
        const renderFooterCell = (
          header: Header<TData, unknown>,
          colIndex: number,
        ) => (
          <TableCell
            key={header.id}
            colSpan={header.colSpan}
            style={{
              ...getWidthStyle(header.column, table),
              ...getColumnPinningStyle(header.column),
            }}
            data-striped={
              isDataTableCellStriped(striping, 0, colIndex, "foot") || undefined
            }
            className={cn(
              padding,
              "bg-background",
              getColumnPinningClass(header.column),
            )}
          >
            {header.isPlaceholder
              ? null
              : flexRender(header.column.columnDef.footer, header.getContext())}
          </TableCell>
        );
        const cells = enableColumnVirtualization
          ? virtualColumns.flatMap((vc) => {
              const header = footerGroup.headers[vc.index];
              return header ? renderFooterCell(header, vc.index) : [];
            })
          : footerGroup.headers.map((header, colIndex) =>
              renderFooterCell(header, colIndex),
            );
        return (
          <TableRow key={footerGroup.id} className="hover:bg-transparent">
            {withColumnSpacers(cells, `footer-${footerGroup.id}`)}
          </TableRow>
        );
      })}
    </TableFooter>
  );
}
