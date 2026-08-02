import { Effect } from "effect";
import { jsPDF } from "jspdf";
import { applyPlugin } from "jspdf-autotable";

import { formatDateTime } from "@/lib/format-date";
import {
  BRAND,
  buildColumnStyles,
  fetchLogoData,
  resolveBrandFromTheme,
} from "@/lib/reports/render-helpers";
import type {
  OrgReportTheme,
  ReportColumn,
  ReportFormatContext,
} from "@/lib/reports/types";

applyPlugin(jsPDF);

const DEFAULT_COMPANY_NAME = "OG Service";

export function generatePdfBlob(
  title: string,
  columns: ReadonlyArray<ReportColumn>,
  rows: string[][],
  ctx: ReportFormatContext,
  orgTheme?: OrgReportTheme,
): Effect.Effect<Blob, Error> {
  return Effect.gen(function* () {
    const logoData = yield* fetchLogoData(orgTheme?.logoUrl);
    const brand = resolveBrandFromTheme(orgTheme);
    const companyName = orgTheme?.name ?? DEFAULT_COMPANY_NAME;

    return yield* Effect.try({
      try: () => {
        const doc = new jsPDF({ orientation: "landscape", unit: "mm" });
        const pageWidth = doc.internal.pageSize.getWidth();
        const pageHeight = doc.internal.pageSize.getHeight();
        const logoBytes = logoData ? new Uint8Array(logoData) : null;
        const now = new Date();

        const drawHeader = () => {
          doc.setFillColor(...brand.primary);
          doc.rect(0, 0, pageWidth, 30, "F");

          let textOffsetX = 10;
          if (logoBytes) {
            Effect.runSync(
              Effect.try({
                try: () => doc.addImage(logoBytes, "PNG", 10, 7, 16, 16),
                catch: () => new Error("Falha ao adicionar logo"),
              }).pipe(Effect.catchAll(() => Effect.void)),
            );
            textOffsetX = 30;
          }

          doc.setTextColor(...BRAND.white);
          doc.setFont("helvetica", "bold");
          doc.setFontSize(16);
          doc.text(companyName, textOffsetX, 13);

          doc.setFont("helvetica", "normal");
          doc.setFontSize(8);
          doc.text("Performance · Segurança · Confiabilidade", textOffsetX, 19);

          doc.setFont("helvetica", "bold");
          doc.setFontSize(13);
          doc.text(title, pageWidth - 10, 13, {
            align: "right",
          });
        };

        drawHeader();

        doc.autoTable({
          head: [columns.map((column) => column.label)],
          body: rows,
          startY: 40,
          styles: { fontSize: 9, cellPadding: 2 },
          headStyles: {
            fillColor: brand.primary,
            textColor: BRAND.white,
            fontStyle: "bold",
            fontSize: 9,
          },
          alternateRowStyles: { fillColor: BRAND.light },
          columnStyles: buildColumnStyles(columns),
          margin: { top: 45, bottom: 20, left: 10, right: 10 },
          didDrawPage: (data) => {
            drawHeader();

            if (data.pageNumber === 1) {
              doc.setTextColor(...BRAND.grey);
              doc.setFont("helvetica", "normal");
              doc.setFontSize(8);
              const metaText = `Gerado: ${formatDateTime(now, ctx.timezone, ctx.datetimeFormat)}  ·  Total: ${rows.length} medição(ões)`;
              doc.text(metaText, 10, 36);
            }

            const footerY = pageHeight - 15;
            doc.setDrawColor(...brand.primary);
            doc.setLineWidth(0.5);
            doc.line(10, footerY, pageWidth - 10, footerY);

            doc.setTextColor(...BRAND.grey);
            doc.setFont("helvetica", "normal");
            doc.setFontSize(7);
            doc.text(
              `© ${now.getFullYear()} ${companyName} · comercial@ogservice.com.br · (71) 9 9615-1703`,
              10,
              footerY + 5,
            );
            const pageCount = doc.getNumberOfPages();
            doc.text(
              `Página ${data.pageNumber} de ${pageCount}`,
              pageWidth - 10,
              footerY + 5,
              { align: "right" },
            );
          },
        });

        return doc.output("blob");
      },
      catch: (error) =>
        error instanceof Error ? error : new Error("Falha ao gerar PDF"),
    });
  });
}
