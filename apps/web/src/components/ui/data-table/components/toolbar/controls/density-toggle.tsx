import type { RowData } from "@tanstack/react-table";

import { Button } from "@/components/ui/button";
import { DENSITY_ORDER } from "@/components/ui/data-table/core/constants";
import type { DataTableInstance } from "@/components/ui/data-table/core/types";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

const DENSITY_LABEL_KEYS = {
  comfortable: "densityComfortable",
  compact: "densityCompact",
  spacious: "densitySpacious",
} as const;

/** Single button that cycles comfortable → compact → spacious. */
export function DataTableDensityToggle<TData extends RowData>({
  table,
}: {
  table: DataTableInstance<TData>;
}) {
  const { localization, icons, density, setDensity } = table.tableInstance;
  const currentLabel = localization[DENSITY_LABEL_KEYS[density]];
  const label = `${localization.toggleDensity} (${currentLabel})`;
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="outline"
            size="icon"
            aria-label={label}
            className="size-8"
            onClick={() =>
              setDensity((prev) => {
                const next =
                  DENSITY_ORDER[
                    (DENSITY_ORDER.indexOf(prev) + 1) % DENSITY_ORDER.length
                  ];
                return next ?? "comfortable";
              })
            }
          />
        }
      >
        <icons.density />
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
