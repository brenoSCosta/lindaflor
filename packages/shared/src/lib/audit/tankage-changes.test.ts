import { describe, expect, it } from "bun:test";

import { buildTankageAuditChanges } from "@lindaflor/shared/lib/audit/tankage-changes";

describe("buildTankageAuditChanges", () => {
  it("returns only changed editable fields", () => {
    const measuredAt = new Date("2026-07-25T12:00:00.000Z");
    const nextMeasuredAt = new Date("2026-07-25T13:00:00.000Z");

    const changes = buildTankageAuditChanges({
      before: {
        measured_at: measuredAt,
        current_measurement: 3.2,
        oil_temperature_c: 40,
        ambient_temperature_c: 30,
        observation: "ok",
        operator_user_id: "op-1",
        measurement_equipment_id: "eq-1",
      },
      after: {
        measured_at: nextMeasuredAt,
        current_measurement: 3.45,
        oil_temperature_c: 40,
        ambient_temperature_c: 30,
        observation: "ok",
        operator_user_id: "op-1",
        measurement_equipment_id: "eq-1",
      },
    });

    expect(changes).toEqual([
      {
        field: "measured_at",
        from: "2026-07-25T12:00:00.000Z",
        to: "2026-07-25T13:00:00.000Z",
      },
      {
        field: "current_measurement",
        from: 3.2,
        to: 3.45,
      },
    ]);
  });

  it("skips fields absent from after snapshot", () => {
    const changes = buildTankageAuditChanges({
      before: {
        current_measurement: 1,
        observation: "a",
      },
      after: {
        current_measurement: 2,
      },
    });

    expect(changes).toEqual([
      {
        field: "current_measurement",
        from: 1,
        to: 2,
      },
    ]);
  });

  it("treats null and undefined observation as equal when both empty", () => {
    const changes = buildTankageAuditChanges({
      before: {
        observation: null,
      },
      after: {
        observation: null,
      },
    });
    expect(changes).toEqual([]);
  });
});
