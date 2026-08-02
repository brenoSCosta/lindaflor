import type { DataTableStriping } from "@/components/ui/data-table/core/types";

export type DataTableStripeRegion = "body" | "head" | "foot";

export function isDataTableCellStriped(
  striping: DataTableStriping | undefined,
  rowIndex: number,
  colIndex: number,
  region: DataTableStripeRegion,
): boolean {
  if (!striping) return false;
  if (striping === "row") {
    return region === "body" && rowIndex % 2 === 1;
  }
  if (striping === "column") {
    return colIndex % 2 === 1;
  }
  if (region === "body") {
    return (rowIndex + colIndex) % 2 === 1;
  }
  return colIndex % 2 === 1;
}
