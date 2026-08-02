import type { Header, RowData } from "@tanstack/react-table";

import { CheckboxFilterField } from "@/components/ui/data-table/components/head/filter-variants/checkbox";
import { DateFilterField } from "@/components/ui/data-table/components/head/filter-variants/date";
import { DateRangeFilterField } from "@/components/ui/data-table/components/head/filter-variants/date-range";
import { MultiSelectFilterField } from "@/components/ui/data-table/components/head/filter-variants/multi-select";
import { NumberFilterField } from "@/components/ui/data-table/components/head/filter-variants/number";
import { RangeSliderFilterField } from "@/components/ui/data-table/components/head/filter-variants/range-slider";
import { SelectFilterField } from "@/components/ui/data-table/components/head/filter-variants/select";
import type { FilterFieldProps } from "@/components/ui/data-table/components/head/filter-variants/shared";
import { TextFilterField } from "@/components/ui/data-table/components/head/filter-variants/text";
import { DataTableFilterModeMenu } from "@/components/ui/data-table/components/menus/data-table-filter-mode-menu";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";

interface DataTableColumnFilterProps<TData extends RowData, TValue> {
  header: Header<TData, TValue>;
  table: DataTableInstance<TData>;
}

/**
 * One filter-row field. Reads `column.meta.variant` and renders the matching
 * control, preceded by the filter-mode menu (where the variant supports modes).
 * `meta.renderColumnFilter` is an escape hatch for fully custom UI. Returns an
 * empty placeholder for non-filterable columns so the grid stays aligned.
 */
export function DataTableColumnFilter<TData extends RowData, TValue>({
  header,
  table,
}: DataTableColumnFilterProps<TData, TValue>) {
  const { column } = header;

  if (header.isPlaceholder || !column.getCanFilter()) {
    return <div className="h-8" />;
  }

  const custom = column.columnDef.meta?.renderColumnFilter;
  if (custom) {
    return <>{custom({ column, table })}</>;
  }

  return (
    <div className="flex items-center gap-0.5">
      <DataTableFilterModeMenu column={column} table={table} />
      <div className="min-w-0 flex-1">
        <FilterField column={column} table={table} />
      </div>
    </div>
  );
}

function FilterField<TData extends RowData, TValue>({
  column,
  table,
}: FilterFieldProps<TData, TValue>) {
  const variant = column.columnDef.meta?.variant ?? "text";
  switch (variant) {
    case "select":
      return <SelectFilterField column={column} table={table} />;
    case "multi-select":
      return <MultiSelectFilterField column={column} table={table} />;
    case "checkbox":
      return <CheckboxFilterField column={column} table={table} />;
    case "range":
      return <NumberFilterField column={column} table={table} />;
    case "range-slider":
      return <RangeSliderFilterField column={column} table={table} />;
    case "date":
      return <DateFilterField column={column} table={table} />;
    case "date-range":
      return <DateRangeFilterField column={column} table={table} />;
    default:
      return <TextFilterField column={column} table={table} />;
  }
}
