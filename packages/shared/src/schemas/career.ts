import { z } from "zod";

export const CAREER_TYPES = ["CLT", "PJ", "Estágio", "Temporário"] as const;
export type CareerType = (typeof CAREER_TYPES)[number];

const careerOutput = z.object({
  id: z.guid(),
  title: z.string(),
  department: z.string(),
  location: z.string(),
  type: z.enum(CAREER_TYPES),
  description: z.string().nullable(),
  requirements: z.array(z.string()),
  is_active: z.boolean(),
  created_at: z.date(),
});
export type CareerOutput = z.infer<typeof careerOutput>;

export const schema = {
  v1: {
    list: {
      input: z.object({
        onlyActive: z.boolean().optional(),
      }),
      output: z.object({
        data: z.array(careerOutput),
      }),
    },

    create: {
      input: z.object({
        title: z.string().min(1, { error: "Título é obrigatório" }),
        department: z.string().min(1, { error: "Departamento é obrigatório" }),
        location: z.string().min(1, { error: "Localidade é obrigatória" }),
        type: z.enum(CAREER_TYPES, {
          error: "Selecione o regime de contratação",
        }),
        description: z.string().optional(),
        requirements: z
          .string()
          .optional()
          .transform((val) =>
            val
              ? val.split(",").flatMap((s) => {
                  const trimmed = s.trim();
                  return trimmed ? [trimmed] : [];
                })
              : [],
          ),
        is_active: z.boolean().optional(),
      }),
      output: careerOutput,
    },

    update: {
      input: z.object({
        id: z.guid(),
        title: z.string().min(1).optional(),
        department: z.string().min(1).optional(),
        location: z.string().min(1).optional(),
        type: z.enum(CAREER_TYPES).optional(),
        description: z.string().optional(),
        requirements: z
          .string()
          .optional()
          .transform((val) =>
            val
              ? val.split(",").flatMap((s) => {
                  const trimmed = s.trim();
                  return trimmed ? [trimmed] : [];
                })
              : undefined,
          ),
        is_active: z.boolean().optional(),
      }),
      output: careerOutput,
    },

    delete: {
      input: z.object({
        id: z.guid(),
      }),
      output: z.null(),
    },
  },
};
