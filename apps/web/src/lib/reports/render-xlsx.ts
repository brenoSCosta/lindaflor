import { write as writeWorkbook, utils as xlsxUtils } from "xlsx";

import type { ReportColumn } from "@/lib/reports/types";

export function generateXlsxBlob(
  title: string,
  columns: ReadonlyArray<ReportColumn>,
  rows: string[][],
): Blob {
  const titleRow = [`OG Service — ${title}`];
  const headerRow = columns.map((column) => column.label);

  const aoa: (string | number)[][] = [titleRow, headerRow, ...rows];

  const worksheet = xlsxUtils.aoa_to_sheet(aoa);

  if (!worksheet["!ref"]) {
    const range = xlsxUtils.encode_range({
      s: { r: 0, c: 0 },
      e: { r: aoa.length - 1, c: columns.length - 1 },
    });
    worksheet["!ref"] = range;
  }

  worksheet["!merges"] = [
    {
      s: { r: 0, c: 0 },
      e: { r: 0, c: Math.max(0, columns.length - 1) },
    },
  ];

  const colWidths = columns.map((column, index) => {
    const headerLen = column.label.length;
    let maxLen = headerLen;
    for (const row of rows) {
      const cellValue = row[index] ?? "";
      maxLen = Math.max(maxLen, cellValue.length);
    }
    return { wch: Math.min(Math.max(maxLen + 2, 10), 40) };
  });
  worksheet["!cols"] = colWidths;

  worksheet["!freeze"] = { ySplit: 2 };

  const workbook = xlsxUtils.book_new();
  const sheetName = title.length > 31 ? title.slice(0, 31) : title;
  xlsxUtils.book_append_sheet(workbook, worksheet, sheetName);
  const array = writeWorkbook(workbook, { bookType: "xlsx", type: "array" });
  return new Blob([array], {
    type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  });
}
