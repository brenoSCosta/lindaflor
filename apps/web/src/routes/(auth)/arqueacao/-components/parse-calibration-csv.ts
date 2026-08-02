/**
 * Parse arqueação CSV with columns height_cm,volume_m3.
 * Accepts `,` or `;` as delimiter. With `;`, `,` may be used as decimal.
 * Headers may also use altura/height and volume/vol.
 */

/** Sample file for the "Baixar exemplo" action (1 cm steps, 150 points). */
const CALIBRATION_CSV_EXAMPLE_ROW_COUNT = 150;

function buildCalibrationCsvExample(): string {
  const lines = ["height_cm,volume_m3"];
  for (let cm = 1; cm <= CALIBRATION_CSV_EXAMPLE_ROW_COUNT; cm += 1) {
    const volume_m3 = Number(((cm / 100) * 18.5).toFixed(4));
    lines.push(`${cm},${volume_m3}`);
  }
  return `${lines.join("\n")}\n`;
}

export const CALIBRATION_CSV_EXAMPLE = buildCalibrationCsvExample();

export const CALIBRATION_CSV_EXAMPLE_FILENAME = "exemplo-tabela-arqueacao.csv";

export function downloadCalibrationCsvExample(): void {
  if (typeof document === "undefined") return;
  const blob = new Blob([CALIBRATION_CSV_EXAMPLE], {
    type: "text/csv;charset=utf-8",
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = CALIBRATION_CSV_EXAMPLE_FILENAME;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

function parseCsvNumber(raw: string): number | null {
  const normalized = raw.trim().replace(/\s/g, "").replace(",", ".");
  if (normalized === "") return null;
  const value = Number(normalized);
  return Number.isFinite(value) ? value : null;
}

export function parseCalibrationCsv(text: string): {
  points: { height_cm: number; volume_m3: number }[];
  error?: string;
} {
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  if (lines.length === 0) {
    return { points: [], error: "Arquivo CSV vazio" };
  }

  const firstLine = lines[0] ?? "";
  const delimiter = firstLine.includes(";") ? ";" : ",";
  const headerCells = firstLine
    .split(delimiter)
    .map((c) => c.trim().toLowerCase());

  let startIndex = 0;
  let heightIdx = 0;
  let volumeIdx = 1;
  const looksLikeHeader =
    headerCells.some((c) => c.includes("height") || c.includes("altura")) &&
    headerCells.some((c) => c.includes("volume") || c.includes("vol"));

  if (looksLikeHeader) {
    startIndex = 1;
    const h = headerCells.findIndex(
      (c) => c.includes("height") || c.includes("altura"),
    );
    const v = headerCells.findIndex(
      (c) => c.includes("volume") || c.includes("vol"),
    );
    if (h >= 0) heightIdx = h;
    if (v >= 0) volumeIdx = v;
  }

  const points: { height_cm: number; volume_m3: number }[] = [];
  for (let i = startIndex; i < lines.length; i++) {
    const line = lines[i] ?? "";
    const cells = line.split(delimiter);
    const heightRaw = cells[heightIdx] ?? "";
    const volumeRaw = cells[volumeIdx] ?? "";
    const height_cm = parseCsvNumber(heightRaw);
    const volume_m3 = parseCsvNumber(volumeRaw);
    if (height_cm == null || volume_m3 == null) {
      return {
        points: [],
        error: `Linha ${i + 1}: altura (cm) e volume (m³) numéricos obrigatórios`,
      };
    }
    if (height_cm <= 0 || volume_m3 <= 0) {
      return {
        points: [],
        error: `Linha ${i + 1}: altura e volume devem ser maiores que zero`,
      };
    }
    points.push({ height_cm, volume_m3 });
  }

  if (points.length === 0) {
    return { points: [], error: "Nenhum ponto encontrado no CSV" };
  }

  const heights = new Set(points.map((p) => p.height_cm));
  if (heights.size !== points.length) {
    return { points: [], error: "Alturas duplicadas no CSV" };
  }

  return { points };
}
