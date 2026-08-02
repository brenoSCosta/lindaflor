import {
  columnFilters,
  facetsSchema,
  globalFilter,
  keepPinnedRows,
  pagination,
  rowPinning,
  sorting,
} from "@lindaflor/shared/lib/utils";
import { z } from "zod";

export const rowSchema = z.object({
  id: z.guid(),
  name: z.string(),
  concession_id: z.guid(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type InstallationOutput = z.infer<typeof rowSchema>;

export const defaultFacets = {
  created_at: { type: "date" as const, label: "Created at" },
  updated_at: { type: "date" as const, label: "Updated at" },
};

export const schema = {
  v1: {
    getAll: {
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

    getById: {
      input: z.object({
        id: z.guid(),
      }),
      output: rowSchema,
    },

    create: {
      input: z.object({
        name: z
          .string()
          .min(1, { error: "Installation name is required" })
          .trim(),
        concession_id: z.guid(),
      }),
      output: rowSchema,
    },

    update: {
      input: z.object({
        id: z.guid(),
        name: z
          .string()
          .min(1, { error: "Installation name is required" })
          .trim()
          .optional(),
        concession_id: z.guid().optional(),
      }),
      output: rowSchema,
    },

    delete: {
      input: z.object({
        ids: z.array(z.guid()).min(1),
      }),
      output: z.null(),
    },
  },
};
