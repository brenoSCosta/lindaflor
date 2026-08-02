import { describe, expect, it } from "bun:test";

import {
  buildBulletinTraceDetail,
  buildBulletinTraceLabel,
} from "@/routes/(auth)/tankages/-components/tank-day-bulletin-trace-labels";

describe("buildBulletinTraceLabel", () => {
  it("formats approve and reopen with actor names", () => {
    expect(
      buildBulletinTraceLabel({
        entity_type: "tank_day_bulletin",
        action: "approve",
        actor_name: "Ana",
      }),
    ).toBe("Aprovado por Ana");
    expect(
      buildBulletinTraceLabel({
        entity_type: "tank_day_bulletin",
        action: "reopen",
        actor_name: "Bruno",
      }),
    ).toBe("Reaberto por Bruno");
  });

  it("formats measurement and transfer actions", () => {
    expect(
      buildBulletinTraceLabel({
        entity_type: "tankage",
        action: "create",
        actor_name: "Ana",
      }),
    ).toBe("Medição de produção criada por Ana");
    expect(
      buildBulletinTraceLabel({
        entity_type: "tank_transfer",
        action: "create",
        actor_name: "Ana",
      }),
    ).toBe("Transferência registrada por Ana");
    expect(
      buildBulletinTraceLabel({
        entity_type: "tankage",
        action: "retreat",
        actor_name: "Ana",
      }),
    ).toBe("Retratamento de medição por Ana");
    expect(
      buildBulletinTraceLabel({
        entity_type: "tank_transfer",
        action: "retreat",
        actor_name: "Bruno",
      }),
    ).toBe("Retratamento de transferência por Bruno");
  });
});

describe("buildBulletinTraceDetail", () => {
  it("formats field changes", () => {
    expect(
      buildBulletinTraceDetail({
        entity_type: "tankage",
        metadata: {
          changes: [
            { field: "current_measurement", from: 3.2, to: 3.45 },
            { field: "oil_temperature_c", from: 40, to: 41 },
          ],
        },
      }),
    ).toBe("Altura 3,2 m → 3,45 m · Temp. óleo 40 °C → 41 °C");
  });

  it("appends justification for retreat changes", () => {
    expect(
      buildBulletinTraceDetail({
        entity_type: "tankage",
        metadata: {
          justification: "Erro de digitação",
          changes: [{ field: "current_measurement", from: 3.2, to: 3.45 }],
        },
      }),
    ).toBe("Altura 3,2 m → 3,45 m · Justificativa: Erro de digitação");
  });

  it("formats transfer metadata", () => {
    expect(
      buildBulletinTraceDetail({
        entity_type: "tank_transfer",
        metadata: {
          height_before_m: 4,
          height_after_m: 2.5,
          gross_volume_out_m3: 12.4,
          destination_label: "Tanque B",
        },
      }),
    ).toBe("Altura 4 m → 2,5 m · 12,4 m³ · para Tanque B");
  });

  it("formats deleted count", () => {
    expect(
      buildBulletinTraceDetail({
        entity_type: "tank_day_bulletin",
        metadata: { deleted_count: 4 },
      }),
    ).toBe("4 medições excluídas");
  });
});
