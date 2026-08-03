import { Effect } from "effect";

const defaultLogoUrl = "https://placehold.co/64x64/f472b6/ffffff?text=LF";
import type {
  OrgReportTheme,
  ReportColumn,
  RgbTriplet,
} from "@/components/ui/data-table/reports/types";

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
