import type { TimeFormatStr } from "@/context/time-format-options";
import type { ReportRegistryMap } from "@/lib/reports/registry";

export type ReportKey = keyof ReportRegistryMap;
export type ReportFormat = "pdf" | "xlsx";
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
 * Workers cannot read React context or cookies, so the selected timezone and
 * format are threaded from {@link useTimezone}/{@link useTimeFormat} into the
 * worker via the spawn message.
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

export interface SpawnReportWorkerMessage {
  readonly key: ReportKey;
  readonly format: ReportFormat;
  readonly rows: readonly object[];
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

export type ReportRowFor<K extends ReportKey> = ReportRegistryMap[K];
