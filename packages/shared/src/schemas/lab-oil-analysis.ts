import { lab_oil_sample_types } from "@lindaflor/shared/enums/tankage";
import { z } from "zod";

export const labOilSampleTypeSchema = z.enum(lab_oil_sample_types);

const data = z.object({
  id: z.guid(),
  tank_id: z.guid(),
  sample_type: labOilSampleTypeSchema,
  collected_at: z.date(),
  issued_at: z.iso.date(),
  certificate_number: z.string(),
  laboratory_name: z.string(),
  method_density: z.string().nullable(),
  method_basic_sediment_water: z.string().nullable(),
  density_at_20c: z.number(),
  water_and_sediment_percent: z.number(),
  salinity: z.number().nullable(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type LabOilAnalysisOutput = z.infer<typeof data>;

const densityAt20c = z
  .number({ error: "Informe a densidade a 20 °C" })
  .positive({ message: "Densidade deve ser maior que zero" });

const waterAndSedimentPercent = z
  .number({ error: "Informe o percentual de água e sedimentos" })
  .min(0, { message: "Água e sedimentos deve ser no mínimo 0%" })
  .max(100, { message: "Água e sedimentos deve ser no máximo 100%" });

const createFields = {
  tank_id: z.guid(),
  sample_type: labOilSampleTypeSchema,
  collected_at: z.date(),
  issued_at: z.iso.date(),
  certificate_number: z
    .string()
    .min(1, { message: "Informe o número do certificado" })
    .trim(),
  laboratory_name: z
    .string()
    .min(1, { message: "Informe o laboratório" })
    .trim(),
  method_density: z.string().trim().nullable().optional(),
  method_basic_sediment_water: z.string().trim().nullable().optional(),
  density_at_20c: densityAt20c,
  water_and_sediment_percent: waterAndSedimentPercent,
  salinity: z.number().nonnegative().nullable().optional(),
};

export const schema = {
  v1: {
    listByTank: {
      input: z.object({
        tank_id: z.guid(),
      }),
      output: z.object({
        data: z.array(data),
      }),
    },

    getById: {
      input: z.object({
        id: z.guid(),
      }),
      output: data,
    },

    create: {
      input: z.object(createFields),
      output: data,
    },

    update: {
      input: z.object({
        id: z.guid(),
        sample_type: labOilSampleTypeSchema.optional(),
        collected_at: z.date().optional(),
        issued_at: z.iso.date().optional(),
        certificate_number: z
          .string()
          .min(1, { message: "Informe o número do certificado" })
          .trim()
          .optional(),
        laboratory_name: z
          .string()
          .min(1, { message: "Informe o laboratório" })
          .trim()
          .optional(),
        method_density: z.string().trim().nullable().optional(),
        method_basic_sediment_water: z.string().trim().nullable().optional(),
        density_at_20c: densityAt20c.optional(),
        water_and_sediment_percent: waterAndSedimentPercent.optional(),
        salinity: z.number().nonnegative().nullable().optional(),
      }),
      output: data,
    },

    delete: {
      input: z.object({
        id: z.guid(),
      }),
      output: z.null(),
    },
  },
};
