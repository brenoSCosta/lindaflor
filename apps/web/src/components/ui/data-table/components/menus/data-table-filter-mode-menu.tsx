import type { Column, RowData } from "@tanstack/react-table";
import { useMemo } from "react";

import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import { modeOptionsForVariant } from "@/components/ui/data-table/fns/variant-modes";
import { getEffectiveMode } from "@/components/ui/data-table/helpers/effective-filter-mode";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

/**
 * The `Filter` adornment that opens a mode menu (contains/equals/…; numeric and
 * date variants get their own sets). Swapping the mode changes the column's
 * `filterFn` and resets the stale value. Hidden when the variant has a single
 * fixed mode or modes are disabled.
 */
export function DataTableFilterModeMenu<TData extends RowData, TValue>({
  column,
  table,
}: {
  column: Column<TData, TValue>;
  table: DataTableInstance<TData>;
}) {
  const {
    localization,
    icons,
    setColumnFilterMode,
    enableColumnFilterModes,
    renderColumnFilterModeMenuItems,
  } = table.tableInstance;
  const variant = column.columnDef.meta?.variant ?? "text";
  const perColumn = column.columnDef.meta?.enableColumnFilterModes;
  const enabled = perColumn ?? enableColumnFilterModes;
  // Restrict (and order) to the column's allowed subset when provided.
  const allowed = column.columnDef.meta?.columnFilterModeOptions;
  const variantModes = modeOptionsForVariant(variant);
  const variantModesSet = useMemo(() => new Set(variantModes), [variantModes]);
  const modes = allowed
    ? allowed.filter((mode) => variantModesSet.has(mode))
    : variantModes;

  if (!enabled || modes.length === 0) return null;

  const currentMode = getEffectiveMode(column, table);

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger
          render={
            <DropdownMenuTrigger
              render={
                <button
                  type="button"
                  aria-label={localization.changeFilterMode}
                  className={cn(
                    "flex size-8 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/40 aria-expanded:text-foreground",
                  )}
                />
              }
            />
          }
        >
          <icons.filter className="size-3.5" />
        </TooltipTrigger>
        <TooltipContent>{localization.filterMode}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="w-52">
        {/* Base UI's GroupLabel requires a Group ancestor; Radix renders the
            group as an inert wrapper. */}
        <DropdownMenuGroup>
          <DropdownMenuLabel>{localization.filterMode}</DropdownMenuLabel>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        {renderColumnFilterModeMenuItems ? (
          renderColumnFilterModeMenuItems({
            column,
            modes,
            currentMode,
            onSelect: (value) => {
              const mode = modes.find((m) => m === value);
              if (mode) setColumnFilterMode(column.id, mode);
            },
            table,
          })
        ) : (
          <DropdownMenuRadioGroup
            value={currentMode}
            onValueChange={(value) => {
              const mode = modes.find((m) => m === value);
              if (mode) setColumnFilterMode(column.id, mode);
            }}
          >
            {modes.map((mode) => (
              <DropdownMenuRadioItem key={mode} value={mode}>
                {localization.filterModes[mode] ?? mode}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
