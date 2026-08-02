import type { RowData } from "@tanstack/react-table";

import { DataTablePagination } from "@/components/ui/data-table/components/toolbar/data-table-pagination";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import { cn } from "@/lib/utils";

interface DataTableBottomToolbarProps<TData extends RowData> {
  table: DataTableInstance<TData>;
  pageSizeOptions?: number[];
}

function RenderBottomToolbar<TData extends RowData>({
  table,
}: {
  table: DataTableInstance<TData>;
}) {
  const { renderBottomToolbar } = table.tableInstance;
  if (!renderBottomToolbar) return null;
  const Component = renderBottomToolbar;
  return <Component table={table} />;
}

/**
 * The bottom toolbar region. Honors a `renderBottomToolbar` override; otherwise
 * lays out optional custom actions alongside the bottom pagination control,
 * rendering nothing when neither is present.
 */
export function DataTableBottomToolbar<TData extends RowData>({
  table,
  pageSizeOptions,
}: DataTableBottomToolbarProps<TData>) {
  const {
    enablePagination,
    positionPagination,
    renderBottomToolbar,
    renderBottomToolbarCustomActions,
    refs: tableRefs,
  } = table.tableInstance;

  if (renderBottomToolbar) return <RenderBottomToolbar table={table} />;

  const customActions = renderBottomToolbarCustomActions?.({ table });
  const showBottomPagination =
    enablePagination &&
    (positionPagination === "bottom" || positionPagination === "both");
  const pagination = showBottomPagination ? (
    <DataTablePagination table={table} pageSizeOptions={pageSizeOptions} />
  ) : null;

  if (customActions == null && pagination == null) return null;
  const bottomToolbarRef = tableRefs?.tableHeadRef;

  return (
    <div
      ref={bottomToolbarRef}
      data-slot="data-table-bottom-toolbar"
      className={cn(
        customActions != null &&
          "flex flex-wrap items-center justify-between gap-4",
      )}
    >
      {customActions != null ? (
        <>
          <div className="flex items-center gap-2">{customActions}</div>
          {pagination && <div className="flex-1">{pagination}</div>}
        </>
      ) : (
        pagination
      )}
    </div>
  );
}
