import { describe, expect, it } from "bun:test";

import { pickVigentCalibrationByTank } from "@lindaflor/shared/functions/tankage/vigent-calibration";

const hasPoints = (id: string) => id === "full";

describe("pickVigentCalibrationByTank", () => {
  it("picks earliest valid_from among overlapping vigent certificates", () => {
    const rows = [
      {
        id: "new",
        tank_id: "t1",
        valid_from: "2024-06-01",
        valid_until: null,
      },
      {
        id: "old",
        tank_id: "t1",
        valid_from: "2020-01-01",
        valid_until: null,
      },
    ];
    const picked = pickVigentCalibrationByTank(rows, "2025-01-01");
    expect(picked.get("t1")?.id).toBe("old");
  });

  it("ignores expired and future certificates", () => {
    const rows = [
      {
        id: "future",
        tank_id: "t1",
        valid_from: "2030-01-01",
        valid_until: null,
      },
      {
        id: "expired",
        tank_id: "t1",
        valid_from: "2018-01-01",
        valid_until: "2019-12-31",
      },
      {
        id: "current",
        tank_id: "t1",
        valid_from: "2022-01-01",
        valid_until: null,
      },
    ];
    const picked = pickVigentCalibrationByTank(rows, "2025-01-01");
    expect(picked.get("t1")?.id).toBe("current");
  });

  it("prefers earliest vigent certificate that has points", () => {
    const rows = [
      {
        id: "empty",
        tank_id: "t1",
        valid_from: "2020-01-01",
        valid_until: null,
      },
      {
        id: "full",
        tank_id: "t1",
        valid_from: "2024-01-01",
        valid_until: null,
      },
    ];

    const picked = pickVigentCalibrationByTank(rows, "2025-01-01", {
      hasPoints,
    });
    expect(picked.get("t1")?.id).toBe("full");
  });
});
