import { z } from "zod";

const AVATAR_MAX_SIZE_BYTES = 2 * 1024 * 1024;

export const rowSchema = z.object({
  id: z.guid(),
  name: z.string(),
  email: z.string(),
  email_verified: z.boolean(),
  image: z.string().nullable(),
  created_at: z.date(),
  updated_at: z.date(),
  two_factor_enabled: z.boolean().nullable(),
  role: z.string().nullable(),
  banned: z.boolean().nullable(),
  ban_reason: z.string().nullable(),
  ban_expires: z.date().nullable(),
});
export type UserOutput = z.infer<typeof rowSchema>;

export const schema = {
  v1: {
    avatar: {
      update: {
        input: z.object({
          file: z
            .file({ error: "Selecione uma imagem" })
            .mime(["image/jpeg", "image/png", "image/webp"], {
              error: "Apenas imagens JPG, PNG ou WebP são permitidas",
            })
            .max(AVATAR_MAX_SIZE_BYTES, {
              error: "A imagem deve ter no máximo 2MB",
            }),
        }),
        output: z.object({
          image: z.string(),
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
