import React from "react";

import type {
  DataTableColumnVirtualizer,
  DataTableRowVirtualizer,
} from "@/components/ui/data-table/core/types";

interface DataTableVirtualizerRefSyncProps {
  rowVirtualizer: DataTableRowVirtualizer;
  columnVirtualizer: DataTableColumnVirtualizer;
  onRowVirtualizerInstance?: (instance: DataTableRowVirtualizer) => void;
  onColumnVirtualizerInstance?: (instance: DataTableColumnVirtualizer) => void;
}

/** Notifies optional listeners when virtualizer instances change. */
export function DataTableVirtualizerRefSync({
  rowVirtualizer,
  columnVirtualizer,
  onRowVirtualizerInstance,
  onColumnVirtualizerInstance,
}: DataTableVirtualizerRefSyncProps) {
  React.useLayoutEffect(() => {
    onRowVirtualizerInstance?.(rowVirtualizer);
    onColumnVirtualizerInstance?.(columnVirtualizer);
  }, [
    onRowVirtualizerInstance,
    onColumnVirtualizerInstance,
    rowVirtualizer,
    columnVirtualizer,
  ]);

  return null;
}
