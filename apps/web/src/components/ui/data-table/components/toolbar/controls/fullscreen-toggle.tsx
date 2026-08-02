import type { RowData } from "@tanstack/react-table";

import { Button } from "@/components/ui/button";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

/** Full-screen toggle (state-driven; the surface fixes itself to the viewport). */
export function DataTableFullscreenToggle<TData extends RowData>({
  table,
}: {
  table: DataTableInstance<TData>;
}) {
  const { localization, icons, isFullscreen, setIsFullscreen } =
    table.tableInstance;
  const label = isFullscreen
    ? localization.exitFullscreen
    : localization.enterFullscreen;
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="outline"
            size="icon"
            aria-label={label}
            aria-pressed={isFullscreen}
            onClick={() => setIsFullscreen((prev) => !prev)}
            className="size-8"
          />
        }
      >
        {isFullscreen ? <icons.fullscreenExit /> : <icons.fullscreenEnter />}
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
