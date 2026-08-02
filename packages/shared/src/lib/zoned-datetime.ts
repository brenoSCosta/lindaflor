import { formatInTimeZone, fromZonedTime } from "date-fns-tz";

export interface ZonedParts {
  dayKey: string;
  hour: string;
  minute: string;
}

export function zonedParts(date: Date, timezone: string): ZonedParts {
  return {
    dayKey: formatInTimeZone(date, timezone, "yyyy-MM-dd"),
    hour: formatInTimeZone(date, timezone, "HH"),
    minute: formatInTimeZone(date, timezone, "mm"),
  };
}

export function zonedDateTimeToUtc(
  dayKey: string,
  hour: string,
  minute: string,
  timezone: string,
): Date {
  return fromZonedTime(`${dayKey}T${hour}:${minute}:00`, timezone);
}

export function dayKeyToCalendarDate(dayKey: string, timezone: string): Date {
  return fromZonedTime(`${dayKey}T12:00:00`, timezone);
}

export function calendarDateToDayKey(date: Date, timezone: string): string {
  return formatInTimeZone(date, timezone, "yyyy-MM-dd");
}

export function nowInTimezone(timezone: string): Date {
  const { dayKey, hour, minute } = zonedParts(new Date(), timezone);
  return zonedDateTimeToUtc(dayKey, hour, minute, timezone);
}

/** Calendar day (yyyy-MM-dd) for business logic, in the given IANA timezone. */
export function operationalDayKey(measuredAt: Date, timezone: string): string {
  return formatInTimeZone(measuredAt, timezone, "yyyy-MM-dd");
}
