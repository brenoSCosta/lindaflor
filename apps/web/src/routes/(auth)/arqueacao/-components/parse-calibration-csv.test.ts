import { describe, expect, it } from "bun:test";

import {
  CALIBRATION_CSV_EXAMPLE,
  parseCalibrationCsv,
} from "@/routes/(auth)/arqueacao/-components/parse-calibration-csv";

describe("parseCalibrationCsv", () => {
  it("parses the downloadable example CSV", () => {
    const result = parseCalibrationCsv(CALIBRATION_CSV_EXAMPLE);
    expect(result.error).toBeUndefined();
    expect(result.points).toHaveLength(150);
    expect(result.points[0]).toEqual({ height_cm: 1, volume_m3: 0.185 });
    expect(result.points[149]).toEqual({ height_cm: 150, volume_m3: 27.75 });
  });

  it("parses comma-delimited CSV with header", () => {
    const result = parseCalibrationCsv(
      "height_cm,volume_m3\n1,0.5\n100,10\n150,16\n",
    );
    expect(result.error).toBeUndefined();
    expect(result.points).toEqual([
      { height_cm: 1, volume_m3: 0.5 },
      { height_cm: 100, volume_m3: 10 },
      { height_cm: 150, volume_m3: 16 },
    ]);
  });

  it("parses semicolon CSV with decimal comma", () => {
    const result = parseCalibrationCsv("altura;volume\n1;0,5\n150,5;16,2\n");
    expect(result.error).toBeUndefined();
    expect(result.points).toEqual([
      { height_cm: 1, volume_m3: 0.5 },
      { height_cm: 150.5, volume_m3: 16.2 },
    ]);
  });

  it("rejects zero height or volume", () => {
    expect(parseCalibrationCsv("height_cm,volume_m3\n0,1\n").error).toMatch(
      /maiores que zero/i,
    );
    expect(parseCalibrationCsv("height_cm,volume_m3\n1,0\n").error).toMatch(
      /maiores que zero/i,
    );
  });

  it("rejects duplicates", () => {
    const result = parseCalibrationCsv("1,1\n100,10\n100,12\n");
    expect(result.error).toMatch(/duplicad/i);
  });
});
