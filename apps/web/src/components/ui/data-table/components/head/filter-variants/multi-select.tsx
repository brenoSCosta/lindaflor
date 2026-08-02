import type { RowData } from "@tanstack/react-table";
import { useMemo } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  FIELD_CLASS,
  useSelectOptions,
  type FilterFieldProps,
} from "@/components/ui/data-table/components/head/filter-variants/shared";
import { getColumnLabel } from "@/components/ui/data-table/helpers/column-label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";

export function MultiSelectFilterField<TData extends RowData, TValue>({
  column,
  table,
}: FilterFieldProps<TData, TValue>) {
  const { localization } = table.tableInstance;
  const { options, counts } = useSelectOptions(column);
  const raw = column.getFilterValue();
  const selected = useMemo(
    () =>
      Array.isArray(raw) && raw.every((v): v is string => typeof v === "string")
        ? raw
        : [],
    [raw],
  );
  const selectedSet = useMemo(() => new Set(selected), [selected]);

  const toggle = (value: string) => {
    const next = selectedSet.has(value)
      ? selected.filter((v) => v !== value)
      : [...selected, value];
    column.setFilterValue(next.length > 0 ? next : undefined);
  };

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            variant="outline"
            size="sm"
            className={cn(
              FIELD_CLASS,
              "w-full justify-between px-2 font-normal",
            )}
            aria-label={localization.filterByColumn(getColumnLabel(column))}
          />
        }
      >
        {selected.length > 0 ? (
          <span className="flex min-w-0 items-center gap-1">
            <Badge variant="secondary" className="rounded-sm px-1">
              {selected.length}
            </Badge>
            <span className="truncate">{selected.join(", ")}</span>
          </span>
        ) : (
          <span className="truncate text-muted-foreground">
            {localization.filterPlaceholder(getColumnLabel(column))}
          </span>
        )}
      </PopoverTrigger>
      <PopoverContent className="w-56 p-0" align="start">
        <Command>
          <CommandInput placeholder={localization.search} className="h-8" />
          <CommandList>
            <CommandEmpty>{localization.noRecordsToDisplay}</CommandEmpty>
            <CommandGroup>
              {options.map((option) => {
                const isSelected = selectedSet.has(option.value);
                return (
                  <CommandItem
                    key={option.value}
                    value={option.value}
                    onSelect={() => toggle(option.value)}
                    className="gap-2"
                  >
                    <Checkbox
                      checked={isSelected}
                      className="pointer-events-none"
                    />
                    <span className="flex-1 truncate">{option.label}</span>
                    <span className="text-xs text-muted-foreground tabular-nums">
                      {counts.get(option.value) ?? 0}
                    </span>
                  </CommandItem>
                );
              })}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
