import { describe, expect, it } from "bun:test";

import {
  schema,
  tankageCurrentMeasurementSchema,
  withTankageMeasurementValidation,
} from "@lindaflor/shared/schemas/tankage/tankages";
import { z } from "zod";

const TANK_ID = "3f2a1c7e-1c4e-4c6b-9f0a-2b7d5e8a1c11";
const OPERATOR_ID = "6b1d4e9a-2f3c-4d5e-8a7b-9c0d1e2f3a44";

function createInput(overrides: Record<string, unknown> = {}) {
  return {
    tank_id: TANK_ID,
    operator_user_id: OPERATOR_ID,
    measured_at: new Date("2026-07-25T12:00:00.000Z"),
    current_measurement: 2.5,
    oil_temperature_c: 32.5,
    ambient_temperature_c: 28,
    observation: "Medição de rotina",
    ...overrides,
  };
}

describe("tankage create input", () => {
  it("accepts a valid reading and trims the observation", () => {
    const result = schema.create.input.safeParse(
      createInput({ observation: "  Medição de rotina  " }),
    );
    expect(result.success).toBe(true);
    expect(result.data?.observation).toBe("Medição de rotina");
  });

  it("rejects a whitespace-only observation", () => {
    const result = schema.create.input.safeParse(
      createInput({ observation: "   " }),
    );
    expect(result.success).toBe(false);
    expect(result.error?.issues[0]?.message).toBe("Informe a observação");
  });

  it("reports a missing height instead of a range error", () => {
    const result = schema.create.input.safeParse(
      createInput({ current_measurement: Number.NaN }),
    );
    expect(result.success).toBe(false);
    expect(result.error?.issues[0]?.message).toBe("Informe a altura");
  });

  it("rejects a negative height", () => {
    const result = schema.create.input.safeParse(
      createInput({ current_measurement: -0.1 }),
    );
    expect(result.success).toBe(false);
  });

  it("rejects implausible temperatures", () => {
    expect(
      schema.create.input.safeParse(createInput({ oil_temperature_c: 400 }))
        .success,
    ).toBe(false);
    expect(
      schema.create.input.safeParse(
        createInput({ ambient_temperature_c: -120 }),
      ).success,
    ).toBe(false);
  });

  it("rejects coordinates outside the globe", () => {
    expect(
      schema.create.input.safeParse(createInput({ latitude: -91 })).success,
    ).toBe(false);
    expect(
      schema.create.input.safeParse(createInput({ longitude: 181 })).success,
    ).toBe(false);
  });
});

describe("tankage retreat input", () => {
  const id = "9d8c7b6a-5e4f-4a3b-8c2d-1e0f9a8b7c66";

  it("rejects a whitespace-only justification", () => {
    const result = schema.retreat.input.safeParse({
      id,
      current_measurement: 3,
      justification: "   ",
    });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0]?.message).toBe(
      "Informe a justificativa do retratamento",
    );
  });

  it("accepts a correction with a justification", () => {
    const result = schema.retreat.input.safeParse({
      id,
      current_measurement: 3,
      justification: "Erro de digitação na altura",
    });
    expect(result.success).toBe(true);
  });
});

describe("tankageCurrentMeasurementSchema", () => {
  it("reports a single message for a negative height", () => {
    const result = tankageCurrentMeasurementSchema({
      previous_measurement: 1.5,
      capacity_height_m: 3,
    }).safeParse(-1);

    expect(result.success).toBe(false);
    expect(result.error?.issues).toHaveLength(1);
    expect(result.error?.issues[0]?.message).toBe(
      "A altura não pode ser negativa",
    );
  });
});

describe("withTankageMeasurementValidation", () => {
  const base = z.object({
    measured_at: z.date(),
    current_measurement: z.number(),
  });

  it("maps height and time issues onto field paths", () => {
    const previous = new Date("2026-07-25T10:00:00.000Z");
    const validated = withTankageMeasurementValidation(base, () => ({
      previous_measurement: 2,
      capacity_height_m: 5,
      previous_measured_at: previous,
      next_measured_at: null,
    }));

    const result = validated.safeParse({
      measured_at: new Date("2026-07-25T09:00:00.000Z"),
      current_measurement: 1,
    });

    expect(result.success).toBe(false);
    const paths = result.error?.issues.map((issue) => issue.path.join("."));
    expect(paths).toContain("current_measurement");
    expect(paths).toContain("measured_at");
  });

  it("allows a height drop when allowDecrease is set", () => {
    const validated = withTankageMeasurementValidation(
      base,
      () => ({
        previous_measurement: 2,
        capacity_height_m: 5,
        previous_measured_at: null,
        next_measured_at: null,
      }),
      { allowDecrease: true },
    );

    expect(
      validated.safeParse({
        measured_at: new Date("2026-07-25T12:00:00.000Z"),
        current_measurement: 1,
      }).success,
    ).toBe(true);
  });
});
