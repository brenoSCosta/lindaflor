import type { ReportAlign, ReportFormatContext } from "@/lib/reports/types";

export interface ReportDefinition<TRow> {
  readonly title: string;
  readonly columns: ReadonlyArray<{
    id: keyof TRow;
    label: string;
    align?: ReportAlign;
  }>;
  readonly format: (
    row: TRow,
    columnId: keyof TRow,
    ctx: ReportFormatContext,
  ) => string | undefined;
}

export interface ReportRegistryMap {}

export interface StoredDefinition {
  readonly title: string;
  readonly columns: ReadonlyArray<{
    id: string;
    label: string;
    align?: ReportAlign;
  }>;
  readonly format: (
    row: Record<string, unknown>,
    columnId: string,
    ctx: ReportFormatContext,
  ) => string | undefined;
}

const definitions = new Map<string, StoredDefinition>();

export function registerReport<TRow>(
  key: string,
  definition: ReportDefinition<TRow>,
): void {
  if (definitions.has(key)) {
    throw new Error(`Report key already registered: ${key}`);
  }
  definitions.set(key, {
    title: definition.title,
    columns: definition.columns.map((column) => ({
      id: String(column.id),
      label: column.label,
      align: column.align,
    })),
    format: (row, columnId, ctx) =>
      definition.format(
        row as unknown as TRow,
        columnId as unknown as keyof TRow,
        ctx,
      ),
  });
}

export function getDefinition(key: string): StoredDefinition {
  const entry = definitions.get(key);
  if (!entry) {
    throw new Error(`Unknown report key: ${key}`);
  }
  return entry;
}
