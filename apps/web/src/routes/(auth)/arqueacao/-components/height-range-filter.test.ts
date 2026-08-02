import { describe, expect, it } from "bun:test";

import { newCalibrationPoint } from "@/routes/(auth)/arqueacao/-components/calibration-points";
import {
  ALL_HEIGHT_BANDS_KEY,
  buildHeightBands,
  heightBandKey,
  heightBounds,
  rangeForBandKey,
  resolveSelectedBandKey,
} from "@/routes/(auth)/arqueacao/-components/height-range-filter";

describe("heightBounds", () => {
  it("returns defaults when there are no points", () => {
    expect(heightBounds([])).toEqual({ min: 0, max: 100 });
  });

  it("expands single height by 1 for max", () => {
    expect(heightBounds([newCalibrationPoint(50, 0)])).toEqual({
      min: 50,
      max: 51,
    });
  });
});

describe("buildHeightBands", () => {
  it("returns empty when min > max", () => {
    expect(buildHeightBands(10, 5, 30)).toEqual([]);
  });

  it("builds 30 cm bands from 0 through 100", () => {
    const bands = buildHeightBands(0, 100, 30);
    expect(bands).toHaveLength(4);
    expect(bands[0]).toMatchObject({ from: 0, to: 30 });
    expect(bands[1]).toMatchObject({ from: 30, to: 60 });
    expect(bands[2]).toMatchObject({ from: 60, to: 90 });
    expect(bands[3]).toMatchObject({ from: 90, to: 100 });
  });

  it("clips first band to data min when not aligned", () => {
    const bands = buildHeightBands(5, 100, 30);
    expect(bands[0]).toMatchObject({ from: 5, to: 30 });
  });

  it("handles a single partial band", () => {
    const bands = buildHeightBands(12, 20, 30);
    expect(bands).toEqual([expect.objectContaining({ from: 12, to: 20 })]);
  });
});

describe("resolveSelectedBandKey", () => {
  it("keeps a valid band key", () => {
    const bands = buildHeightBands(0, 100, 30);
    const band = bands[1];
    expect(band).toBeDefined();
    if (band == null) return;
    const key = heightBandKey(band.from, band.to);
    expect(resolveSelectedBandKey(key, bands)).toBe(key);
  });

  it("falls back to all when the key is stale", () => {
    expect(resolveSelectedBandKey("0:30", [])).toBe(ALL_HEIGHT_BANDS_KEY);
  });
});

describe("rangeForBandKey", () => {
  it("returns the full range for all", () => {
    expect(rangeForBandKey(ALL_HEIGHT_BANDS_KEY, [], [10, 90])).toEqual([
      10, 90,
    ]);
  });

  it("returns the band range for a band key", () => {
    const bands = buildHeightBands(0, 100, 30);
    const band = bands[2];
    expect(band).toBeDefined();
    if (band == null) return;
    const key = heightBandKey(band.from, band.to);
    expect(rangeForBandKey(key, bands, [0, 100])).toEqual([60, 90]);
  });
});
