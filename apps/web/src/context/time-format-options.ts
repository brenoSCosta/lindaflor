/**
 * Allowed date-fns format strings shared by the {@link Time} component and the
 * report workers so on-screen rendering and exported documents stay consistent.
 * See https://date-fns.org/v4.1.0/docs/format
 *
 * Each entry pairs a token with a concrete sample (the label) and a category,
 * so the selector dropdown can group options and show a human-readable preview
 * instead of the raw token.
 */
type TimeFormatGroup = "datetime" | "date" | "time";

const TIME_FORMAT_ENTRIES = [
  {
    value: "dd-MM-yyyy HH:mm:ss",
    group: "datetime",
    label: "14-02-2025 15:30:00",
  },
  { value: "dd/MM/yyyy HH:mm", group: "datetime", label: "14/02/2025 15:30" },
  {
    value: "yyyy-MM-dd'T'HH:mm:ss",
    group: "datetime",
    label: "2025-02-14T15:30:00",
  },
  { value: "Pp", group: "datetime", label: "02/14/2025, 3:30 PM" },
  { value: "PPpp", group: "datetime", label: "Feb 14, 2025, 3:30:00 PM" },
  {
    value: "PPPppp",
    group: "datetime",
    label: "February 14th, 2025 at 3:30:00 PM GMT+1",
  },
  {
    value: "PPPPpppp",
    group: "datetime",
    label: "Friday, February 14th, 2025 at 3:30:00 PM GMT+01:00",
  },
  { value: "yyyy-MM-dd", group: "date", label: "2025-02-14" },
  { value: "dd/MM/yyyy", group: "date", label: "14/02/2025" },
  { value: "MMM d, yyyy", group: "date", label: "Feb 14, 2025" },
  { value: "MMM d", group: "date", label: "Feb 14" },
  { value: "EEE", group: "date", label: "Fri" },
  { value: "EEEE", group: "date", label: "Friday" },
  { value: "P", group: "date", label: "02/14/2025" },
  { value: "PP", group: "date", label: "Feb 14, 2025" },
  { value: "PPP", group: "date", label: "February 14th, 2025" },
  { value: "PPPP", group: "date", label: "Friday, February 14th, 2025" },
  { value: "HH:mm", group: "time", label: "15:30" },
  { value: "p", group: "time", label: "3:30 PM" },
  { value: "pp", group: "time", label: "3:30:00 PM" },
  { value: "ppp", group: "time", label: "3:30:00 PM GMT+1" },
  { value: "pppp", group: "time", label: "3:30:00 PM GMT+01:00" },
] as const satisfies readonly {
  readonly value: string;
  readonly group: TimeFormatGroup;
  readonly label: string;
}[];

export type TimeFormatStr = (typeof TIME_FORMAT_ENTRIES)[number]["value"];

/**
 * Narrows a runtime value (e.g. one that crossed the worker postMessage
 * boundary as a plain string) to a {@link TimeFormatStr}, falling back to
 * {@link DEFAULT_TIME_FORMAT} when unknown. Avoids unsafe type assertions on
 * data crossing a runtime boundary.
 */
export function coerceTimeFormatStr(value: unknown): TimeFormatStr {
  if (typeof value !== "string") return DEFAULT_TIME_FORMAT;
  const found = TIME_FORMAT_ENTRIES.find((entry) => entry.value === value);
  return found?.value ?? DEFAULT_TIME_FORMAT;
}

/**
 * Default format used when the user has not selected one. Also resolves the
 * pre-existing inconsistency between the Time component default
 * ("dd-MM-yyyy HH:mm:ss") and the report default ("dd/MM/yyyy HH:mm").
 */
export const DEFAULT_TIME_FORMAT: TimeFormatStr = "dd/MM/yyyy HH:mm";

export const CLIENT_TIME_FORMAT_COOKIE_NAME = "client-time-format";

export type TimeFormatOption = (typeof TIME_FORMAT_ENTRIES)[number];

/**
 * Selectable format options, each shown as a concrete sample so users can pick
 * by example rather than by date-fns token.
 */
export const TIME_FORMAT_OPTIONS: readonly TimeFormatOption[] =
  TIME_FORMAT_ENTRIES;

/** Display labels for each {@link TimeFormatGroup}, in Portuguese. */
export const TIME_FORMAT_GROUP_LABELS: Record<TimeFormatGroup, string> = {
  datetime: "Data e hora",
  date: "Data",
  time: "Hora",
};

/**
 * Options partitioned by group, in dropdown render order. Each group keeps its
 * entries in the order declared in {@link TIME_FORMAT_ENTRIES}.
 */
export const TIME_FORMAT_OPTION_GROUPS: ReadonlyArray<{
  readonly group: TimeFormatGroup;
  readonly options: readonly TimeFormatOption[];
}> = [
  {
    group: "datetime",
    options: TIME_FORMAT_OPTIONS.filter((o) => o.group === "datetime"),
  },
  {
    group: "date",
    options: TIME_FORMAT_OPTIONS.filter((o) => o.group === "date"),
  },
  {
    group: "time",
    options: TIME_FORMAT_OPTIONS.filter((o) => o.group === "time"),
  },
];
