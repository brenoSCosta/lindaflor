import type { TimeFormatStr } from "@/context/time-format-options";

export type ReportFormat = "pdf" | "xlsx" | "csv";
export type ReportAlign = "left" | "right" | "center";

export type RgbTriplet = [number, number, number];

/**
 * Organization-specific theme for PDF reports.
 * Passed from the main thread (which has React context) into the Web Worker.
 */
export interface OrgReportTheme {
  readonly name: string;
  readonly logoUrl: string | null;
  readonly primary: RgbTriplet;
  readonly secondary: RgbTriplet;
}

/**
 * Resolved datetime preferences for formatting date/datetime report cells.
 * Workers cannot read React context or cookies, so timezone and format are
 * threaded from the main thread into the worker via the spawn message.
 */
export interface ReportFormatContext {
  readonly timezone: string;
  readonly datetimeFormat: TimeFormatStr;
}

export interface ReportColumn {
  readonly id: string;
  readonly label: string;
  readonly align?: ReportAlign;
}

/** Inline-only spawn payload: main thread already formatted cells. */
export interface SpawnReportWorkerMessage {
  readonly format: ReportFormat;
  readonly title: string;
  readonly columns: readonly ReportColumn[];
  readonly rows: readonly string[][];
  readonly timezone: string;
  /** date-fns format string; crosses postMessage boundary as a plain string. */
  readonly datetimeFormat: string;
  readonly orgTheme?: OrgReportTheme;
}

export interface ReportReadyMessage {
  readonly type: "ready";
  readonly blob: Blob;
}

export interface ReportErrorMessage {
  readonly type: "error";
  readonly message: string;
}

export interface ReportProgressMessage {
  readonly type: "progress";
  readonly percent: number;
  readonly step: string;
}

export type ReportWorkerMessage =
  | ReportReadyMessage
  | ReportErrorMessage
  | ReportProgressMessage;
