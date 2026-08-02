import { tank_day_bulletin_statuses } from "@lindaflor/shared/enums/tankage";
import { jsonInstantSchema } from "@lindaflor/shared/lib/json-instant";
import {
  columnFilters,
  facetsSchema,
  globalFilter,
  keepPinnedRows,
  pagination,
  rowPinning,
  rowSelection,
  sorting,
  type FacetsSchema,
} from "@lindaflor/shared/lib/utils";
import { z } from "zod";

export const rawRowSchema = z.object({
  id: z.guid(),
  tank_id: z.guid(),
  concession_id: z.guid(),
  installation_id: z.guid(),
  measurement_equipment_id: z.guid().nullable(),
  operator_user_id: z.guid(),
  measured_at: z.date(),
  operational_day: z.iso.date(),
  previous_measurement: z.number(),
  current_measurement: z.number(),
  oil_temperature_c: z.number(),
  ambient_temperature_c: z.number(),
  observation: z.string(),
  latitude: z.number().nullable(),
  longitude: z.number().nullable(),
  gross_volume_m3: z.number().nullable(),
  gross_volume_m3_20c: z.number().nullable(),
  net_oil_volume_m3_20c: z.number().nullable(),
  volume_oil_barrels: z.number().nullable(),
  shell_temperature_c: z.number().nullable(),
  shell_correction_factor: z.number().nullable(),
  liquid_correction_factor: z.number().nullable(),
  combined_correction_factor: z.number().nullable(),
  tank_calibration_id: z.guid().nullable(),
  lab_oil_analysis_id: z.guid().nullable(),
  density_at_20c_kg_m3: z.number().nullable(),
  water_and_sediment_percent: z.number().nullable(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type TankageSyncRow = z.infer<typeof rawRowSchema>;

const rowSchema = rawRowSchema.extend({
  tag: z.string(),
  concession_name: z.string(),
  installation_name: z.string(),
  measurement_equipment_code: z.string().nullable(),
  operator_name: z.string(),
  bulletin_status: z.enum(tank_day_bulletin_statuses),
});
export type TankageOutput = z.infer<typeof rowSchema>;

export const defaultFacets: FacetsSchema = {
  concession_id: {
    type: "select",
    label: "Concession",
    options: [],
  },
  installation_id: {
    type: "select",
    label: "Installation",
    options: [],
  },
  operator_user_id: {
    type: "select",
    label: "Operator",
    options: [],
  },
  measured_at: { type: "date", label: "Measured at" },
  created_at: { type: "date", label: "Created at" },
  updated_at: { type: "date", label: "Updated at" },
};

function formatMeters(value: number): string {
  return value.toFixed(3);
}

/** Sanity ceiling for a gauge reading; the vigente calibration table is the real limit. */
const MAX_GAUGE_HEIGHT_M = 100;
/** Plausible range for crude oil inside a (possibly heated) storage tank. */
const OIL_TEMPERATURE_C = { min: -50, max: 150 };
/** Plausible ambient range at any installation. */
const AMBIENT_TEMPERATURE_C = { min: -60, max: 60 };
const MAX_TEXT_LENGTH = 1000;

/**
 * Gauge height in meters. Range checks abort so a bounds-aware refinement added on
 * top (previous reading / capacity) does not report a second, redundant message.
 */
const heightMetersSchema = z
  .number({ error: "Informe a altura" })
  .min(0, { error: "A altura não pode ser negativa", abort: true })
  .max(MAX_GAUGE_HEIGHT_M, {
    error: `A altura não pode exceder ${MAX_GAUGE_HEIGHT_M} m`,
    abort: true,
  });

const oilTemperatureSchema = z
  .number({ error: "Informe a temperatura do óleo" })
  .min(OIL_TEMPERATURE_C.min, {
    error: `A temperatura do óleo não pode ser menor que ${OIL_TEMPERATURE_C.min} °C`,
  })
  .max(OIL_TEMPERATURE_C.max, {
    error: `A temperatura do óleo não pode exceder ${OIL_TEMPERATURE_C.max} °C`,
  });

const ambientTemperatureSchema = z
  .number({ error: "Informe a temperatura ambiente" })
  .min(AMBIENT_TEMPERATURE_C.min, {
    error: `A temperatura ambiente não pode ser menor que ${AMBIENT_TEMPERATURE_C.min} °C`,
  })
  .max(AMBIENT_TEMPERATURE_C.max, {
    error: `A temperatura ambiente não pode exceder ${AMBIENT_TEMPERATURE_C.max} °C`,
  });

// Trim before length checks so whitespace-only text is rejected.
const observationSchema = z
  .string({ error: "Informe a observação" })
  .trim()
  .min(1, { error: "Informe a observação" })
  .max(MAX_TEXT_LENGTH, {
    error: `A observação deve ter no máximo ${MAX_TEXT_LENGTH} caracteres`,
  });

const justificationSchema = z
  .string({ error: "Informe a justificativa do retratamento" })
  .trim()
  .min(1, { error: "Informe a justificativa do retratamento" })
  .max(MAX_TEXT_LENGTH, {
    error: `A justificativa deve ter no máximo ${MAX_TEXT_LENGTH} caracteres`,
  });

const latitudeSchema = z
  .number()
  .min(-90, { error: "Latitude deve estar entre -90 e 90" })
  .max(90, { error: "Latitude deve estar entre -90 e 90" });

const longitudeSchema = z
  .number()
  .min(-180, { error: "Longitude deve estar entre -180 e 180" })
  .max(180, { error: "Longitude deve estar entre -180 e 180" });

/**
 * Height limits for a production tankage reading, resolved from the authoritative
 * server state (immediately preceding reading and the vigente calibration table).
 */
export type TankageMeasurementBounds = {
  /** Height (m) of the immediately preceding reading; production must not drop below it. */
  previous_measurement: number;
  /** Maximum gauge height (m) from the calibration table valid on the measurement day. */
  capacity_height_m: number | null;
};

/**
 * Timestamp window for a tankage reading; `measured_at` must fall strictly between
 * the preceding and following points so readings cannot be reordered or duplicated.
 */
export type TankageTimeWindow = {
  /** measured_at must be strictly after this point (previous reading), when present. */
  previous_measured_at: Date | null;
  /** measured_at must be strictly before this point (next reading), when present. */
  next_measured_at: Date | null;
};

/** Full validation context shared by the API guards and the web UI. */
export type TankageMeasurementContext = TankageMeasurementBounds &
  TankageTimeWindow;

/**
 * Zod schema for a production height: never below the previous reading and never
 * above the tank's maximum gauge height. Shared by the API and the web forms.
 *
 * Set `requireCapacity: false` for transfer-originated rows that only need the
 * capacity ceiling when a vigente table exists.
 */
export function tankageCurrentMeasurementSchema(
  bounds: TankageMeasurementBounds,
  options?: { requireCapacity?: boolean },
) {
  const requireCapacity = options?.requireCapacity ?? true;
  return heightMetersSchema.superRefine((value, ctx) => {
    if (value < bounds.previous_measurement) {
      ctx.addIssue({
        code: "custom",
        message: `A altura não pode ser menor que a medição anterior (${formatMeters(bounds.previous_measurement)} m)`,
      });
      return;
    }
    if (bounds.capacity_height_m == null) {
      if (requireCapacity) {
        ctx.addIssue({
          code: "custom",
          message:
            "Tanque sem altura máxima de arqueação vigente — cadastre a tabela de arqueação",
        });
      }
      return;
    }
    if (value > bounds.capacity_height_m) {
      ctx.addIssue({
        code: "custom",
        message: `A altura não pode exceder a altura máxima do tanque (${formatMeters(bounds.capacity_height_m)} m)`,
      });
    }
  });
}

/**
 * Attach neighbor/capacity height and timestamp checks onto any Zod object that
 * already includes `measured_at` and `current_measurement`. Forms can pass the
 * result directly to TanStack Form (`validators: { onSubmit: schema }`) instead
 * of chaining `safeParse` calls by hand.
 *
 * Issues are remapped onto the field paths so field-level errors light up.
 *
 * Set `allowDecrease` for transfer-originated rows (height may drop).
 */
export function withTankageMeasurementValidation<
  Schema extends z.ZodType<{
    measured_at: Date;
    current_measurement: number;
  }>,
>(
  base: Schema,
  getContext: (value: z.output<Schema>) => TankageMeasurementContext,
  options?: { allowDecrease?: boolean },
) {
  return base.superRefine((value, ctx) => {
    const context = getContext(value);
    const heightBounds: TankageMeasurementBounds = {
      previous_measurement: options?.allowDecrease
        ? 0
        : context.previous_measurement,
      capacity_height_m: context.capacity_height_m,
    };

    const height = tankageCurrentMeasurementSchema(heightBounds, {
      requireCapacity: !options?.allowDecrease,
    }).safeParse(value.current_measurement);
    if (!height.success) {
      for (const issue of height.error.issues) {
        ctx.addIssue({
          code: "custom",
          message: issue.message,
          path: ["current_measurement"],
        });
      }
    }

    const time = tankageMeasuredAtSchema(context).safeParse(value.measured_at);
    if (!time.success) {
      for (const issue of time.error.issues) {
        ctx.addIssue({
          code: "custom",
          message: issue.message,
          path: ["measured_at"],
        });
      }
    }
  });
}

/**
 * Zod schema for a reading timestamp constrained to the strict interval between the
 * previous and next points, preventing hour manipulation that would reorder readings.
 */
export function tankageMeasuredAtSchema(window: TankageTimeWindow) {
  return z
    .date({ error: "Informe a hora" })
    .refine(
      (value) =>
        window.previous_measured_at == null ||
        value.getTime() > window.previous_measured_at.getTime(),
      { error: "A hora deve ser posterior à medição anterior" },
    )
    .refine(
      (value) =>
        window.next_measured_at == null ||
        value.getTime() < window.next_measured_at.getTime(),
      { error: "A hora deve ser anterior à próxima medição" },
    );
}

type NeighborRow = {
  id: string;
  measured_at: Date;
  current_measurement: number;
  previous_measurement: number;
};

/**
 * Derive height/time bounds from data the client already has (day rows + snapshot
 * capacity / current height). No extra API call.
 */
export function tankageBoundsFromDayRows(args: {
  rows: readonly NeighborRow[];
  measuredAt: Date;
  capacityHeightM: number | null;
  /** Latest tank height overall (snapshot); used when the day has no prior row. */
  fallbackPreviousHeightM?: number | null;
  excludeId?: string;
}): TankageMeasurementContext {
  const others = args.rows
    .filter((row) => row.id !== args.excludeId)
    .slice()
    // Avoid Array.prototype.toSorted — not available on Hermes used by Expo.
    // oxlint-disable-next-line unicorn/no-array-sort
    .sort((a, b) => a.measured_at.getTime() - b.measured_at.getTime());

  // Hermes lacks Array.prototype.findLast; walk ascending and keep the last match.
  let previous: NeighborRow | undefined;
  for (const row of others) {
    if (row.measured_at.getTime() < args.measuredAt.getTime()) {
      previous = row;
    } else {
      break;
    }
  }
  const next = others.find(
    (row) => row.measured_at.getTime() > args.measuredAt.getTime(),
  );
  const firstOfDay = others[0];

  let previousMeasurement = 0;
  if (previous != null) {
    previousMeasurement = previous.current_measurement;
  } else if (firstOfDay != null && args.excludeId == null) {
    previousMeasurement = firstOfDay.previous_measurement;
  } else if (args.excludeId != null) {
    const editing = args.rows.find((row) => row.id === args.excludeId);
    previousMeasurement = editing?.previous_measurement ?? 0;
  } else {
    previousMeasurement = args.fallbackPreviousHeightM ?? 0;
  }

  return {
    previous_measurement: previousMeasurement,
    capacity_height_m: args.capacityHeightM,
    previous_measured_at: previous?.measured_at ?? null,
    next_measured_at: next?.measured_at ?? null,
  };
}

export const schema = {
  list: {
    all: {
      input: z
        .object({
          pagination,
          sorting,
          columnFilters,
          globalFilter,
          rowPinning,
          keepPinnedRows,
        })
        .optional(),
      output: z.object({
        data: z.array(rowSchema),
        meta: z
          .object({
            rowCount: z.number(),
            facets: facetsSchema,
          })
          .optional(),
      }),
    },
  },

  listBy: {
    tank: {
      input: z.object({
        tank_id: z.guid(),
        measured_on: z.iso.date().optional(),
      }),
      output: z.object({
        data: z.array(rowSchema),
      }),
    },
  },

  getBy: {
    id: {
      input: z.object({
        id: z.guid(),
      }),
      output: rowSchema,
    },
  },

  get: {
    selected: {
      input: z
        .object({
          all: z.literal(true).optional(),
          exclude: z.array(z.guid()).optional(),
          selection: rowSelection.optional(),
          columnFilters: columnFilters.optional(),
          globalFilter: globalFilter.optional(),
          sorting: sorting.optional(),
        })
        .refine(
          (value) =>
            value.all ||
            (value.selection && Object.keys(value.selection).length > 0),
          { message: "Either all or selection must be provided" },
        ),
      output: z.array(rowSchema),
    },
  },

  create: {
    input: z.object({
      tank_id: z.guid(),
      operator_user_id: z.guid(),
      measured_at: jsonInstantSchema(),
      current_measurement: heightMetersSchema,
      oil_temperature_c: oilTemperatureSchema,
      ambient_temperature_c: ambientTemperatureSchema,
      observation: observationSchema,
      previous_measurement: heightMetersSchema.optional(),
      measurement_equipment_id: z.guid().nullable().optional(),
      latitude: latitudeSchema.optional(),
      longitude: longitudeSchema.optional(),
    }),
    output: rowSchema,
  },

  update: {
    input: z.object({
      id: z.guid(),
      tank_id: z.guid().optional(),
      operator_user_id: z.guid().optional(),
      measured_at: z.date({ error: "Informe a hora" }).optional(),
      previous_measurement: heightMetersSchema.optional(),
      current_measurement: heightMetersSchema.optional(),
      oil_temperature_c: oilTemperatureSchema.optional(),
      ambient_temperature_c: ambientTemperatureSchema.optional(),
      observation: observationSchema.optional(),
      measurement_equipment_id: z.guid().nullable().optional(),
      latitude: latitudeSchema.optional(),
      longitude: longitudeSchema.optional(),
    }),
    output: rowSchema,
  },

  retreat: {
    input: z
      .object({
        id: z.guid(),
        measured_at: z.date({ error: "Informe a hora" }).optional(),
        current_measurement: heightMetersSchema.optional(),
        oil_temperature_c: oilTemperatureSchema.optional(),
        ambient_temperature_c: ambientTemperatureSchema.optional(),
        observation: observationSchema.optional(),
        justification: justificationSchema,
      })
      .refine(
        (value) =>
          value.measured_at != null ||
          value.current_measurement != null ||
          value.oil_temperature_c != null ||
          value.ambient_temperature_c != null ||
          value.observation != null,
        { message: "Informe ao menos um campo para corrigir" },
      ),
    output: rowSchema,
  },

  delete: {
    input: z.object({
      ids: z.array(z.guid()).min(1),
    }),
    output: z.null(),
  },
};
