import type { RowData } from "@tanstack/react-table";

import { Button } from "@/components/ui/button";
import {
  FIELD_CLASS,
  useSelectOptions,
  type FilterFieldProps,
} from "@/components/ui/data-table/components/head/filter-variants/shared";
import { getColumnLabel } from "@/components/ui/data-table/helpers/column-label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "@/lib/utils";

export function SelectFilterField<TData extends RowData, TValue>({
  column,
  table,
}: FilterFieldProps<TData, TValue>) {
  const { localization, icons } = table.tableInstance;
  const { options } = useSelectOptions(column);
  const raw = column.getFilterValue();
  const value = typeof raw === "string" ? raw : "";
  return (
    <div className="flex items-center gap-1">
      <Select
        // Always controlled: "" shows the placeholder in both Radix and Base
        // UI, while `undefined` would flip to uncontrolled and go stale.
        value={value}
        onValueChange={(next) => column.setFilterValue(next || undefined)}
      >
        <SelectTrigger
          size="sm"
          className={cn(FIELD_CLASS, "w-full min-w-0 flex-1 px-2")}
          aria-label={localization.filterByColumn(getColumnLabel(column))}
        >
          <SelectValue
            placeholder={localization.filterPlaceholder(getColumnLabel(column))}
          />
        </SelectTrigger>
        <SelectContent>
          {options.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {value && (
        <Button
          variant="ghost"
          size="icon"
          aria-label={localization.clearFilter}
          onClick={() => column.setFilterValue(undefined)}
          className="size-7"
        >
          <icons.clear />
        </Button>
      )}
    </div>
  );
}
