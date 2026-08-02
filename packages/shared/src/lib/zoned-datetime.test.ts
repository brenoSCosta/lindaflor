import { describe, expect, test } from "bun:test";

import {
  calendarDateToDayKey,
  dayKeyToCalendarDate,
  nowInTimezone,
  operationalDayKey,
  zonedDateTimeToUtc,
  zonedParts,
} from "@lindaflor/shared/lib/zoned-datetime";

describe("zoned-datetime", () => {
  test("zonedDateTimeToUtc maps São Paulo wall clock to UTC instant", () => {
    const utc = zonedDateTimeToUtc(
      "2026-07-19",
      "14",
      "30",
      "America/Sao_Paulo",
    );
    expect(utc.toISOString()).toBe("2026-07-19T17:30:00.000Z");
  });

  test("zonedParts round-trips through zonedDateTimeToUtc", () => {
    const instant = zonedDateTimeToUtc(
      "2026-07-19",
      "23",
      "45",
      "America/Sao_Paulo",
    );
    expect(zonedParts(instant, "America/Sao_Paulo")).toEqual({
      dayKey: "2026-07-19",
      hour: "23",
      minute: "45",
    });
  });

  test("calendarDateToDayKey uses timezone not browser-local interpretation", () => {
    const anchor = dayKeyToCalendarDate("2026-07-19", "America/Sao_Paulo");
    expect(calendarDateToDayKey(anchor, "America/Sao_Paulo")).toBe(
      "2026-07-19",
    );
  });

  test("late evening in São Paulo stays on the same local calendar day", () => {
    const instant = zonedDateTimeToUtc(
      "2026-07-19",
      "23",
      "30",
      "America/Sao_Paulo",
    );
    expect(instant.toISOString()).toBe("2026-07-20T02:30:00.000Z");
    expect(zonedParts(instant, "America/Sao_Paulo").dayKey).toBe("2026-07-19");
  });

  test("UTC timezone preserves wall clock digits in parts", () => {
    const instant = zonedDateTimeToUtc("2026-01-15", "08", "00", "UTC");
    expect(zonedParts(instant, "UTC")).toEqual({
      dayKey: "2026-01-15",
      hour: "08",
      minute: "00",
    });
  });

  test("nowInTimezone matches current zoned parts", () => {
    const timezone = "Pacific/Honolulu";
    const instant = nowInTimezone(timezone);
    const parts = zonedParts(new Date(), timezone);
    expect(zonedParts(instant, timezone)).toEqual(parts);
  });
});

describe("operationalDayKey", () => {
  test("late evening in São Paulo maps to the same local calendar day", () => {
    const timezone = "America/Sao_Paulo";
    const measuredAt = zonedDateTimeToUtc("2026-07-19", "23", "30", timezone);
    expect(operationalDayKey(measuredAt, timezone)).toBe("2026-07-19");
  });
});
