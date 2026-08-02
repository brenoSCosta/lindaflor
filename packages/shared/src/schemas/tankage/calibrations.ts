import { z } from "zod";

const pointInput = z.object({
  height_cm: z.number().positive({ message: "Altura deve ser maior que 0 cm" }),
  volume_m3: z.number().positive({ message: "Volume deve ser maior que 0" }),
});

export const rawPointRowSchema = z.object({
  id: z.guid(),
  calibration_id: z.guid(),
  height_cm: z.number(),
  volume_m3: z.number(),
});
export type TankCalibrationPointSyncRow = z.infer<typeof rawPointRowSchema>;

const pointOutput = pointInput.extend({
  id: z.guid(),
  calibration_id: z.guid(),
});

/** Postgres row shape for PowerSync (no derived fields like `is_expired`). */
export const rawRowSchema = z.object({
  id: z.guid(),
  tank_id: z.guid(),
  certificate_number: z.string(),
  issued_at: z.iso.date().nullable(),
  valid_from: z.iso.date(),
  valid_until: z.iso.date().nullable(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type TankCalibrationSyncRow = z.infer<typeof rawRowSchema>;

const calibrationBase = rawRowSchema.extend({
  is_expired: z.boolean(),
});

const listItem = calibrationBase.extend({
  points_count: z.number().int().nonnegative(),
});

const detail = calibrationBase.extend({
  points: z.array(pointOutput),
});

export type TankCalibrationOutput = z.infer<typeof calibrationBase>;
export type TankCalibrationListItem = z.infer<typeof listItem>;
export type TankCalibrationDetail = z.infer<typeof detail>;

const validityRefine = (value: {
  valid_from: string;
  valid_until?: string | null;
}) => value.valid_until == null || value.valid_until >= value.valid_from;

const validityMessage = {
  message: "Validade final deve ser igual ou posterior ao início da vigência",
  path: ["valid_until"] as string[],
};

const uniqueHeights = (points: { height_cm: number }[]) => {
  const seen = new Set<number>();
  for (const point of points) {
    if (seen.has(point.height_cm)) return false;
    seen.add(point.height_cm);
  }
  return true;
};

const currentSummary = z.object({
  tank_id: z.guid(),
  certificate_number: z.string(),
  valid_until: z.iso.date().nullable(),
});

export type TankCalibrationCurrentSummary = z.infer<typeof currentSummary>;

export const schema = {
  list: {
    current: {
      input: z.object({
        at: z.iso.date().optional(),
      }),
      output: z.object({
        data: z.array(currentSummary),
      }),
    },
  },

  listBy: {
    tank: {
      input: z.object({
        tank_id: z.guid(),
      }),
      output: z.object({
        data: z.array(listItem),
      }),
    },
  },

  getBy: {
    id: {
      input: z.object({
        id: z.guid(),
      }),
      output: detail,
    },
  },

  create: {
    input: z
      .object({
        tank_id: z.guid(),
        certificate_number: z
          .string()
          .min(1, { message: "Informe o número do certificado" })
          .trim(),
        issued_at: z.iso.date().nullable().optional(),
        valid_from: z.iso.date(),
        valid_until: z.iso.date().nullable().optional(),
        points: z.array(pointInput).optional(),
      })
      .refine(validityRefine, validityMessage)
      .refine((value) => uniqueHeights(value.points ?? []), {
        message: "Alturas duplicadas na tabela de arqueação",
        path: ["points"],
      }),
    output: detail,
  },

  update: {
    input: z
      .object({
        id: z.guid(),
        certificate_number: z
          .string()
          .min(1, { message: "Informe o número do certificado" })
          .trim()
          .optional(),
        issued_at: z.iso.date().nullable().optional(),
        valid_from: z.iso.date().optional(),
        valid_until: z.iso.date().nullable().optional(),
      })
      .refine(
        (value) =>
          value.valid_from == null ||
          value.valid_until == null ||
          value.valid_until >= value.valid_from,
        validityMessage,
      ),
    output: detail,
  },

  replace: {
    point: {
      input: z
        .object({
          id: z.guid(),
          points: z.array(pointInput).min(1, {
            message: "Informe ao menos um ponto altura/volume",
          }),
        })
        .refine((value) => uniqueHeights(value.points), {
          message: "Alturas duplicadas na tabela de arqueação",
          path: ["points"],
        }),
      output: detail,
    },
  },

  delete: {
    input: z.object({
      ids: z.array(z.guid()).min(1),
    }),
    output: z.null(),
  },

  resolve: {
    volume: {
      input: z.object({
        tank_id: z.guid(),
        /** Field measurement height in metres; converted to cm for table lookup. */
        height_m: z.number().min(0),
        at: z.iso.date(),
      }),
      output: z.object({
        volume_m3: z.number(),
        calibration_id: z.guid(),
        interpolated: z.boolean(),
      }),
    },
  },
};
