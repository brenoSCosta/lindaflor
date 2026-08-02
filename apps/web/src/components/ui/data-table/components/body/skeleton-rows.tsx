import { Skeleton } from "@/components/ui/skeleton";
import { TableCell, TableRow } from "@/components/ui/table";

/** Placeholder skeleton rows shown while the initial data load is in flight. */
export function SkeletonRows({
  rowCount,
  columnCount,
  padding,
}: {
  rowCount: number;
  columnCount: number;
  padding: string;
}) {
  return (
    <>
      {Array.from({ length: Math.max(1, rowCount) }).map((_, rowIndex) => {
        const rowKey = `sk-row-${rowIndex}`;
        return (
          <TableRow key={rowKey} className="hover:bg-transparent">
            {Array.from({ length: Math.max(1, columnCount) }).map(
              (__, colIndex) => {
                const cellKey = `sk-cell-${rowIndex}-${colIndex}`;
                return (
                  <TableCell key={cellKey} className={padding}>
                    <Skeleton className="h-4 w-full max-w-48" />
                  </TableCell>
                );
              },
            )}
          </TableRow>
        );
      })}
    </>
  );
}
