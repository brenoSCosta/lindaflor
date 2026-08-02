import { Effect } from "effect";
import { isRecord } from "effect/Predicate";

const defaultLogoUrl =
  "https://placehold.co/64x64/f472b6/ffffff?text=LF";
import type {
  OrgReportTheme,
  ReportColumn,
  ReportFormatContext,
  RgbTriplet,
} from "@/lib/reports/types";

export const BRAND = {
  primary: [24, 66, 90] as RgbTriplet,
  secondary: [184, 134, 33] as RgbTriplet,
  light: [245, 245, 245] as RgbTriplet,
  grey: [120, 120, 120] as RgbTriplet,
  white: [255, 255, 255] as RgbTriplet,
};

export interface ReportBrand {
  primary: RgbTriplet;
  secondary: RgbTriplet;
}

export function resolveBrandFromTheme(theme?: OrgReportTheme): ReportBrand {
  return {
    primary: theme?.primary ?? BRAND.primary,
    secondary: theme?.secondary ?? BRAND.secondary,
  };
}

const ROW_CHUNK_SIZE = 250;
const ROWS_PROGRESS_START = 20;
const ROWS_PROGRESS_END = 55;

export type PostProgress = (percent: number, step: string) => void;

export function chunkArray<T>(array: readonly T[], size: number): T[][] {
  const chunks: T[][] = [];
  for (let i = 0; i < array.length; i += size) {
    chunks.push(array.slice(i, i + size));
  }
  return chunks;
}

export function buildColumnStyles(
  columns: ReadonlyArray<ReportColumn>,
): Record<number, { halign: "left" | "right" | "center" }> {
  const styles: Record<number, { halign: "left" | "right" | "center" }> = {};
  for (const [index, column] of columns.entries()) {
    if (column.align) {
      styles[index] = { halign: column.align };
    }
  }
  return styles;
}

export function buildReportRowsWithProgress(
  rows: readonly unknown[],
  columns: ReadonlyArray<ReportColumn>,
  ctx: ReportFormatContext,
  formatRow: (
    row: Record<string, unknown>,
    columnId: string,
    ctx: ReportFormatContext,
  ) => string | undefined,
  postProgress?: PostProgress,
): string[][] {
  const result: string[][] = [];
  const span = ROWS_PROGRESS_END - ROWS_PROGRESS_START;
  const total = rows.length;
  for (const [index, chunk] of chunkArray(rows, ROW_CHUNK_SIZE).entries()) {
    if (postProgress && total > ROW_CHUNK_SIZE) {
      const done = Math.min((index + 1) * ROW_CHUNK_SIZE, total);
      const percent = Math.min(
        ROWS_PROGRESS_END,
        ROWS_PROGRESS_START + Math.round((done / total) * span),
      );
      postProgress(percent, `Processando medições… (${done}/${total})`);
    }
    for (const row of chunk) {
      if (!isRecord(row)) {
        continue;
      }
      result.push(
        columns.map((column) => formatRow(row, column.id, ctx) ?? "—"),
      );
    }
  }
  return result;
}

function fetchLogoFromUrl(
  url: string,
): Effect.Effect<ArrayBuffer | null, Error> {
  return Effect.tryPromise({
    try: async () => {
      const response = await fetch(url);
      return await response.arrayBuffer();
    },
    catch: (error) =>
      error instanceof Error ? error : new Error("Falha ao carregar logo"),
  }).pipe(Effect.catchAll(() => Effect.succeed(null)));
}

export const fetchDefaultLogoData = fetchLogoFromUrl(defaultLogoUrl);

export function fetchLogoData(
  logoUrl?: string | null,
): Effect.Effect<ArrayBuffer | null, Error> {
  if (logoUrl) {
    return fetchLogoFromUrl(logoUrl);
  }
  return fetchDefaultLogoData;
}
