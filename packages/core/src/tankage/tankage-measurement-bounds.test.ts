import { describe, expect, test } from "bun:test";

import { assertTankageMeasurementWithinBounds } from "@lindaflor/core/tankage/tankage-measurement-bounds";
import {
  tankageBoundsFromDayRows,
  tankageCurrentMeasurementSchema,
  tankageMeasuredAtSchema,
} from "@lindaflor/shared/schemas/tankage/tankages";
import { ORPCError } from "@orpc/server";

describe("tankageCurrentMeasurementSchema", () => {
  test("allows equal and rising heights within capacity", () => {
    const schema = tankageCurrentMeasurementSchema({
      previous_measurement: 1.5,
      capacity_height_m: 3,
    });
    expect(schema.safeParse(1.5).success).toBe(true);
    expect(schema.safeParse(2).success).toBe(true);
    expect(schema.safeParse(3).success).toBe(true);
  });

  test("rejects below previous", () => {
    const schema = tankageCurrentMeasurementSchema({
      previous_measurement: 1.5,
      capacity_height_m: 3,
    });
    const result = schema.safeParse(1.4);
    expect(result.success).toBe(false);
  });

  test("rejects above capacity", () => {
    const schema = tankageCurrentMeasurementSchema({
      previous_measurement: 0,
      capacity_height_m: 2,
    });
    expect(schema.safeParse(2.001).success).toBe(false);
  });

  test("rejects missing capacity", () => {
    const schema = tankageCurrentMeasurementSchema({
      previous_measurement: 0,
      capacity_height_m: null,
    });
    expect(schema.safeParse(1).success).toBe(false);
  });
});

describe("tankageMeasuredAtSchema", () => {
  const previous = new Date("2026-06-01T12:00:00.000Z");
  const next = new Date("2026-06-01T18:00:00.000Z");

  test("allows strictly between neighbors", () => {
    const schema = tankageMeasuredAtSchema({
      previous_measured_at: previous,
      next_measured_at: next,
    });
    expect(schema.safeParse(new Date("2026-06-01T15:00:00.000Z")).success).toBe(
      true,
    );
  });

  test("rejects equal to previous or next", () => {
    const schema = tankageMeasuredAtSchema({
      previous_measured_at: previous,
      next_measured_at: next,
    });
    expect(schema.safeParse(previous).success).toBe(false);
    expect(schema.safeParse(next).success).toBe(false);
  });

  test("rejects outside range", () => {
    const schema = tankageMeasuredAtSchema({
      previous_measured_at: previous,
      next_measured_at: next,
    });
    expect(schema.safeParse(new Date("2026-06-01T11:59:00.000Z")).success).toBe(
      false,
    );
    expect(schema.safeParse(new Date("2026-06-01T18:01:00.000Z")).success).toBe(
      false,
    );
  });
});

describe("tankageBoundsFromDayRows", () => {
  const rows = [
    {
      id: "a",
      measured_at: new Date("2026-06-01T10:00:00.000Z"),
      current_measurement: 1,
      previous_measurement: 0.5,
    },
    {
      id: "b",
      measured_at: new Date("2026-06-01T14:00:00.000Z"),
      current_measurement: 2,
      previous_measurement: 1,
    },
  ];

  test("uses prior same-day height when inserting between rows", () => {
    const bounds = tankageBoundsFromDayRows({
      rows,
      measuredAt: new Date("2026-06-01T12:00:00.000Z"),
      capacityHeightM: 5,
    });
    expect(bounds.previous_measurement).toBe(1);
    expect(bounds.previous_measured_at?.toISOString()).toBe(
      "2026-06-01T10:00:00.000Z",
    );
    expect(bounds.next_measured_at?.toISOString()).toBe(
      "2026-06-01T14:00:00.000Z",
    );
  });

  test("uses first row previous_measurement when inserting before day rows", () => {
    const bounds = tankageBoundsFromDayRows({
      rows,
      measuredAt: new Date("2026-06-01T08:00:00.000Z"),
      capacityHeightM: 5,
    });
    expect(bounds.previous_measurement).toBe(0.5);
    expect(bounds.previous_measured_at).toBeNull();
    expect(bounds.next_measured_at?.toISOString()).toBe(
      "2026-06-01T10:00:00.000Z",
    );
  });

  test("uses fallback when day is empty", () => {
    const bounds = tankageBoundsFromDayRows({
      rows: [],
      measuredAt: new Date("2026-06-01T12:00:00.000Z"),
      capacityHeightM: 4,
      fallbackPreviousHeightM: 1.25,
    });
    expect(bounds.previous_measurement).toBe(1.25);
    expect(bounds.capacity_height_m).toBe(4);
  });
});

describe("assertTankageMeasurementWithinBounds", () => {
  const context = {
    previous_measurement: 1,
    capacity_height_m: 3,
    previous_measured_at: new Date("2026-06-01T10:00:00.000Z"),
    next_measured_at: new Date("2026-06-01T18:00:00.000Z"),
  };

  test("accepts valid production reading", () => {
    expect(() =>
      assertTankageMeasurementWithinBounds({
        context,
        currentMeasurement: 1.5,
        measuredAt: new Date("2026-06-01T12:00:00.000Z"),
      }),
    ).not.toThrow();
  });

  test("rejects drop for production", () => {
    expect(() =>
      assertTankageMeasurementWithinBounds({
        context,
        currentMeasurement: 0.5,
        measuredAt: new Date("2026-06-01T12:00:00.000Z"),
      }),
    ).toThrow(ORPCError);
  });

  test("allows drop when allowDecrease (transfer)", () => {
    expect(() =>
      assertTankageMeasurementWithinBounds({
        context,
        currentMeasurement: 0.5,
        measuredAt: new Date("2026-06-01T12:00:00.000Z"),
        allowDecrease: true,
      }),
    ).not.toThrow();
  });
});
