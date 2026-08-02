import type { RowData } from "@tanstack/react-table";
import React from "react";

import { Button } from "@/components/ui/button";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import type { GlobalFilterMode } from "@/components/ui/data-table/fns/filter-modes";
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
import { Input } from "@/components/ui/input";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

const GLOBAL_MODES: GlobalFilterMode[] = [
  "fuzzy",
  "contains",
  "startsWith",
  "endsWith",
  "equals",
];

/**
 * Expandable global search (MRT order: first in the toolbar icon cluster). The
 * search button expands into an input with a leading icon, an optional
 * search-mode menu, and a clear affordance. Input is debounced in manual
 * (server) mode so each keystroke doesn't fire a request.
 */
export function DataTableGlobalFilter<TData extends RowData>({
  table,
  searchInputRef,
}: {
  table: DataTableInstance<TData>;
  /** Optional ref forwarded to the search input. Defaults to the instance's
   *  `searchInputRef` when rendered by `DataTable`. */
  searchInputRef?: React.RefObject<HTMLInputElement | null>;
}) {
  const {
    localization,
    icons,
    enableGlobalFilter,
    enableGlobalFilterModes,
    globalFilterMode,
    setGlobalFilterMode,
    renderGlobalFilterModeMenuItems,
  } = table.tableInstance;

  const raw = table.getState().globalFilter;
  const external = typeof raw === "string" ? raw : "";
  const [expanded, setExpanded] = React.useState(external.length > 0);
  const [value, setValue] = React.useState(external);
  // Drive focus-on-expand and let consumers focus the box. Use the forwarded
  // ref when provided (so `DataTable` shares its `searchInputRef`), else a
  // local one. React assigns it directly — no manual `.current` mutation.
  const localRef = React.useRef<HTMLInputElement>(null);
  const inputRef = searchInputRef ?? localRef;

  const debounceMs = table.options.manualFiltering ? 300 : 0;

  React.useEffect(() => {
    const id =
      value === (table.getState().globalFilter ?? "")
        ? undefined
        : setTimeout(
            () => table.setGlobalFilter(value || undefined),
            debounceMs,
          );
    return () => {
      if (id) clearTimeout(id);
    };
  }, [value, debounceMs, table]);

  if (!enableGlobalFilter) return null;

  if (!expanded) {
    return (
      <Tooltip>
        <TooltipTrigger
          render={
            <Button
              variant="outline"
              size="icon"
              aria-label={localization.search}
              className="size-8"
              onClick={() => {
                setExpanded(true);
                requestAnimationFrame(() => inputRef.current?.focus());
              }}
            />
          }
        >
          <icons.search />
        </TooltipTrigger>
        <TooltipContent>{localization.search}</TooltipContent>
      </Tooltip>
    );
  }

  const clear = () => {
    setValue("");
    table.setGlobalFilter(undefined);
    setExpanded(false);
  };

  return (
    <div className="flex h-9 items-center gap-0.5 rounded-md border bg-background pr-1 pl-2 focus-within:border-ring">
      <icons.search className="size-3.5 shrink-0 text-muted-foreground" />
      <Input
        ref={inputRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={() => {
          if (!value) setExpanded(false);
        }}
        placeholder={localization.searchPlaceholder}
        aria-label={localization.search}
        className={cn(
          "h-7 w-40 border-0 px-1 text-xs font-normal tracking-normal normal-case shadow-none focus-visible:ring-0 sm:w-56",
        )}
      />
      {value && (
        <Button
          variant="ghost"
          size="icon"
          aria-label={localization.clearSearch}
          onClick={clear}
          className="size-7"
        >
          <icons.clear />
        </Button>
      )}
      {enableGlobalFilterModes && (
        <DropdownMenu>
          <Tooltip>
            <TooltipTrigger
              render={
                <DropdownMenuTrigger
                  render={
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={localization.globalFilterMode}
                      className="size-7"
                    />
                  }
                />
              }
            >
              <icons.search className="size-3" />
            </TooltipTrigger>
            <TooltipContent>{localization.globalFilterMode}</TooltipContent>
          </Tooltip>
          <DropdownMenuContent align="end" className="w-48">
            {/* Base UI's GroupLabel requires a Group ancestor; Radix renders
                the group as an inert wrapper. */}
            <DropdownMenuGroup>
              <DropdownMenuLabel>
                {localization.globalFilterMode}
              </DropdownMenuLabel>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            {renderGlobalFilterModeMenuItems ? (
              renderGlobalFilterModeMenuItems({
                modes: GLOBAL_MODES,
                currentMode: globalFilterMode,
                onSelect: setGlobalFilterMode,
                table,
              })
            ) : (
              <DropdownMenuRadioGroup
                value={globalFilterMode}
                onValueChange={(mode) => {
                  const next = GLOBAL_MODES.find((m) => m === mode);
                  if (next) setGlobalFilterMode(next);
                }}
              >
                {GLOBAL_MODES.map((mode) => (
                  <DropdownMenuRadioItem key={mode} value={mode}>
                    {localization.filterModes[mode] ?? mode}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
