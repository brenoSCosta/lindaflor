import type { RowData } from "@tanstack/react-table";

import {
  ValuelessLabel,
  type FilterFieldProps,
} from "@/components/ui/data-table/components/head/filter-variants/shared";
import { VALUELESS_MODES } from "@/components/ui/data-table/fns/filter-modes";
import { getColumnLabel } from "@/components/ui/data-table/helpers/column-label";
import { getEffectiveMode } from "@/components/ui/data-table/helpers/effective-filter-mode";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";

export function TextFilterField<TData extends RowData, TValue>({
  column,
  table,
}: FilterFieldProps<TData, TValue>) {
  const { localization, icons } = table.tableInstance;
  const mode = getEffectiveMode(column, table);
  if (VALUELESS_MODES.has(mode)) {
    return <ValuelessLabel label={localization.filterModes[mode] ?? mode} />;
  }
  const value = (() => {
    const raw = column.getFilterValue();
    return typeof raw === "string" ? raw : "";
  })();
  return (
    <InputGroup className="flex-1">
      <InputGroupInput
        value={value}
        onChange={(e) => column.setFilterValue(e.target.value || undefined)}
        placeholder={localization.filterPlaceholder(getColumnLabel(column))}
        aria-label={localization.filterByColumn(getColumnLabel(column))}
      />
      {value && (
        <InputGroupAddon align="inline-end">
          <InputGroupButton
            size="icon-xs"
            variant="ghost"
            aria-label={localization.clearFilter}
            onClick={() => column.setFilterValue(undefined)}
          >
            <icons.clear className="size-4" />
          </InputGroupButton>
        </InputGroupAddon>
      )}
    </InputGroup>
  );
}
