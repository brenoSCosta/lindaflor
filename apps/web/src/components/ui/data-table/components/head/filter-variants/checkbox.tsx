import type { RowData } from "@tanstack/react-table";

import { Checkbox } from "@/components/ui/checkbox";
import type { FilterFieldProps } from "@/components/ui/data-table/components/head/filter-variants/shared";
import { getColumnLabel } from "@/components/ui/data-table/helpers/column-label";

export function CheckboxFilterField<TData extends RowData, TValue>({
  column,
  table,
}: FilterFieldProps<TData, TValue>) {
  const { localization } = table.tableInstance;
  const value = column.getFilterValue();
  const checked = value === true;
  return (
    <label className="flex h-8 items-center gap-2 text-xs text-muted-foreground">
      <Checkbox
        checked={checked}
        onCheckedChange={(next) => column.setFilterValue(next || undefined)}
        aria-label={localization.filterByColumn(getColumnLabel(column))}
      />
      {getColumnLabel(column)}
    </label>
  );
}
