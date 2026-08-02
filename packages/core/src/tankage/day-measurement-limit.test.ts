import { describe, expect, it } from "bun:test";

import { MAX_TANKAGE_MEASUREMENTS_PER_DAY } from "@lindaflor/shared/constants/tankage";

describe("MAX_TANKAGE_MEASUREMENTS_PER_DAY", () => {
  it("is 24", () => {
    expect(MAX_TANKAGE_MEASUREMENTS_PER_DAY).toBe(24);
  });
});
