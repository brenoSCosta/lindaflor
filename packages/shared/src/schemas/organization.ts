import { z } from "zod";

const LOGO_MAX_SIZE_BYTES = 2 * 1024 * 1024;

export const schema = {
  v1: {
    logo: {
      update: {
        input: z.object({
          id: z.guid(),
          file: z
            .file({ error: "Selecione uma imagem" })
            .mime(["image/jpeg", "image/png", "image/webp"], {
              error: "Apenas imagens JPG, PNG ou WebP são permitidas",
            })
            .max(LOGO_MAX_SIZE_BYTES, {
              error: "A imagem deve ter no máximo 2MB",
            }),
        }),
        output: z.object({
          logo: z.string(),
        }),
      },
      get: {
        input: z.object({
          id: z.guid(),
        }),
        output: z.object({
          url: z.string().nullable(),
        }),
      },
    },
  },
};
