import { describe, expect, test } from "bun:test";

import { assertInventoryChangeWithinBounds } from "@lindaflor/core/tankage/tankage-stock-validation";
import { ORPCError } from "@orpc/server";

describe("assertInventoryChangeWithinBounds", () => {
  test("allows fill up to capacity", () => {
    expect(() =>
      assertInventoryChangeWithinBounds({
        priorGrossM3: 10,
        newGrossM3: 100,
        capacityGrossM3: 100,
        hasCalibration: true,
      }),
    ).not.toThrow();
  });

  test("rejects gross above capacity", () => {
    expect(() =>
      assertInventoryChangeWithinBounds({
        priorGrossM3: 10,
        newGrossM3: 101,
        capacityGrossM3: 100,
        hasCalibration: true,
      }),
    ).toThrow(ORPCError);
  });

  test("allows withdrawal up to prior stock", () => {
    expect(() =>
      assertInventoryChangeWithinBounds({
        priorGrossM3: 50,
        newGrossM3: 0,
        capacityGrossM3: 100,
        hasCalibration: true,
      }),
    ).not.toThrow();
  });

  test("rejects height without volume when calibration exists", () => {
    expect(() =>
      assertInventoryChangeWithinBounds({
        priorGrossM3: 50,
        newGrossM3: null,
        capacityGrossM3: 100,
        hasCalibration: true,
      }),
    ).toThrow(ORPCError);
  });

  test("requires documented outflow when transfer cap is set", () => {
    expect(() =>
      assertInventoryChangeWithinBounds({
        priorGrossM3: 50,
        newGrossM3: 40,
        capacityGrossM3: 100,
        hasCalibration: true,
        documentedOutflowGrossM3: 5,
      }),
    ).toThrow(ORPCError);

    expect(() =>
      assertInventoryChangeWithinBounds({
        priorGrossM3: 50,
        newGrossM3: 40,
        capacityGrossM3: 100,
        hasCalibration: true,
        documentedOutflowGrossM3: 15,
      }),
    ).not.toThrow();
  });

  test("rejects unexplained drop when documented outflow is zero", () => {
    expect(() =>
      assertInventoryChangeWithinBounds({
        priorGrossM3: 50,
        newGrossM3: 40,
        capacityGrossM3: 100,
        hasCalibration: true,
        documentedOutflowGrossM3: 0,
      }),
    ).toThrow(ORPCError);
  });
});
