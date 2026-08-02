import type { RowData } from "@tanstack/react-table";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";

/** Toolbar button that opens the advanced filter panel; badges the active-rule
 *  count when any rules are set. */
export function DataTableAdvancedFilterToggle<TData extends RowData>({
  table,
}: {
  table: DataTableInstance<TData>;
}) {
  const {
    localization,
    icons,
    advancedFilter,
    showAdvancedFilterPanel,
    setShowAdvancedFilterPanel,
  } = table.tableInstance;
  const count = advancedFilter.rules.length;

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="outline"
            size="icon"
            aria-label={localization.advancedFilters}
            aria-pressed={showAdvancedFilterPanel}
            onClick={() => setShowAdvancedFilterPanel((prev) => !prev)}
            className={cn(
              "relative size-8",
              (showAdvancedFilterPanel || count > 0) &&
                "bg-muted text-foreground",
            )}
          />
        }
      >
        <icons.advancedFilter />
        {count > 0 && (
          <Badge className="absolute -top-1.5 -right-1.5 size-4 justify-center rounded-full p-0 text-[10px] tabular-nums">
            {count}
          </Badge>
        )}
      </TooltipTrigger>
      <TooltipContent>{localization.advancedFilters}</TooltipContent>
    </Tooltip>
  );
}
