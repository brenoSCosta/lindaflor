import { describe, expect, it } from "bun:test";

import {
  dateRangesOverlap,
  dayBeforeIsoDate,
  interpolateVolume,
  maxCalibrationHeightMFromPoints,
  planOpenEndedClosures,
} from "@lindaflor/shared/functions/tankage/calibration-volume";

const points = [
  { height_cm: 1, volume_m3: 0.5 },
  { height_cm: 100, volume_m3: 10 },
  { height_cm: 200, volume_m3: 22 },
];

describe("maxCalibrationHeightMFromPoints", () => {
  it("returns max height in metres", () => {
    expect(maxCalibrationHeightMFromPoints(points)).toBe(2);
  });

  it("returns null for empty table", () => {
    expect(maxCalibrationHeightMFromPoints([])).toBeNull();
  });
});

describe("interpolateVolume", () => {
  it("returns exact point without interpolation", () => {
    expect(interpolateVolume(points, 100)).toEqual({
      ok: true,
      volume_m3: 10,
      interpolated: false,
    });
  });

  it("interpolates linearly between adjacent points", () => {
    const result = interpolateVolume(points, 150);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.interpolated).toBe(true);
    expect(result.volume_m3).toBeCloseTo(16);
  });

  it("rejects empty table", () => {
    expect(interpolateVolume([], 100).ok).toBe(false);
  });

  it("rejects height below range", () => {
    expect(interpolateVolume(points, 0).ok).toBe(false);
  });

  it("rejects height above range", () => {
    expect(interpolateVolume(points, 201).ok).toBe(false);
  });
});

describe("dateRangesOverlap", () => {
  it("detects overlapping inclusive ranges", () => {
    expect(
      dateRangesOverlap("2024-01-01", "2024-12-31", "2024-06-01", "2025-06-01"),
    ).toBe(true);
  });

  it("allows adjacent non-overlapping ranges", () => {
    expect(
      dateRangesOverlap("2024-01-01", "2024-12-31", "2025-01-01", "2025-12-31"),
    ).toBe(false);
  });

  it("treats null valid_until as open-ended", () => {
    expect(
      dateRangesOverlap("2024-01-01", null, "2025-01-01", "2025-12-31"),
    ).toBe(true);
    expect(
      dateRangesOverlap("2024-01-01", "2024-12-31", "2025-01-01", null),
    ).toBe(false);
  });
});

describe("dayBeforeIsoDate", () => {
  it("returns previous calendar day", () => {
    expect(dayBeforeIsoDate("2025-01-01")).toBe("2024-12-31");
  });
});

describe("planOpenEndedClosures", () => {
  it("closes previous open-ended cert the day before the new one", () => {
    const result = planOpenEndedClosures(
      [{ id: "a", valid_from: "2024-01-01", valid_until: null }],
      "2025-06-01",
      null,
    );
    expect(result.conflict).toBeUndefined();
    expect(result.closes).toEqual([{ id: "a", valid_until: "2025-05-31" }]);
  });

  it("rejects hard overlaps with closed ranges", () => {
    const result = planOpenEndedClosures(
      [{ id: "a", valid_from: "2024-01-01", valid_until: "2025-12-31" }],
      "2025-06-01",
      "2026-01-01",
    );
    expect(result.conflict).toBeDefined();
    expect(result.closes).toEqual([]);
  });
});
