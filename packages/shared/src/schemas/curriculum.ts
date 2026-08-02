import { MAX_FILE_SIZE_BYTES } from "@lindaflor/shared/constants";
import { pagination } from "@lindaflor/shared/lib/utils";
import { z } from "zod";

const curriculumOutput = z.object({
  id: z.guid(),
  name: z.string(),
  email: z.email(),
  phone: z.string().nullable(),
  headline: z.string(),
  summary: z.string().nullable(),
  skills: z.array(z.string()),
  career_id: z.guid().nullable(),
  file_key: z.string(),
  file_name: z.string(),
  file_size: z.number(),
  mime_type: z.string(),
  submitted_at: z.date(),
});
export type CurriculumOutput = z.infer<typeof curriculumOutput>;

export const schema = {
  v1: {
    submit: {
      input: z
        .object({
          name: z.string().min(1, { error: "Nome é obrigatório" }),
          email: z.email({ error: "Email inválido" }),
          phone: z.string().optional(),
          summary: z.string().optional(),
          career_id: z.guid().optional(),
          headline: z.string().optional(),
          skills: z
            .string({ error: "Informe pelo menos uma habilidade" })
            .min(1, { error: "Informe pelo menos uma habilidade" })
            .transform((val) =>
              val.split(",").flatMap((s) => {
                const trimmed = s.trim();
                return trimmed ? [trimmed] : [];
              }),
            )
            .refine((arr) => arr.length > 0, {
              error: "Informe pelo menos uma habilidade",
            }),
          file: z
            .file({ error: "Selecione um arquivo PDF" })
            .mime(["application/pdf"], {
              error: "Apenas arquivos PDF são permitidos",
            })
            .max(MAX_FILE_SIZE_BYTES, {
              error: `O arquivo deve ter no máximo ${MAX_FILE_SIZE_BYTES / (1024 * 1024)}MB`,
            }),
        })
        .superRefine((data, ctx) => {
          if (data.career_id) {
            return;
          }

          if (!data.headline?.trim()) {
            ctx.addIssue({
              code: "custom",
              message: "Informe a área ou cargo de interesse",
              path: ["headline"],
            });
          }
        }),
      output: curriculumOutput,
    },

    list: {
      input: z.object({
        pagination,
        search: z.string().optional(),
      }),
      output: z.object({
        data: z.array(curriculumOutput),
        meta: z.object({
          totalPages: z.number(),
        }),
      }),
    },

    getById: {
      input: z.object({
        id: z.guid(),
      }),
      output: curriculumOutput,
    },

    getDownloadUrl: {
      input: z.object({
        id: z.guid(),
      }),
      output: z.object({
        url: z.string(),
      }),
    },

    delete: {
      input: z.object({
        id: z.guid(),
      }),
      output: z.null(),
    },
  },
};
