import { schema as calibration } from "@lindaflor/shared/schemas/tankage/calibrations";
import { schema as bulletin } from "@lindaflor/shared/schemas/tankage/day-bulletins";
import { schema as summary } from "@lindaflor/shared/schemas/tankage/day-summaries";
import { schema as tankage } from "@lindaflor/shared/schemas/tankage/tankages";
import { schema as tank } from "@lindaflor/shared/schemas/tankage/tanks";
import { schema as transfer } from "@lindaflor/shared/schemas/tankage/transfers";

export const schema = {
  v1: {
    tank,
    tankage,
    transfer,
    calibration,
    bulletin,
    summary,
  },
};
