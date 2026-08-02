import Papa from "papaparse";

import type { ReportColumn } from "@/components/ui/data-table/reports/types";

/** Build a UTF-8 CSV blob (BOM prepended so Excel detects encoding). */
export function generateCsvBlob(
  _title: string,
  columns: ReadonlyArray<ReportColumn>,
  rows: string[][],
): Blob {
  const headerRow = columns.map((column) => column.label);
  const aoa = [headerRow, ...rows];
  const csv = Papa.unparse(aoa);
  return new Blob(["\uFEFF" + csv], { type: "text/csv;charset=utf-8;" });
}
