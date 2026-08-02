import {
  columnFilters,
  facetsSchema,
  globalFilter,
  keepPinnedRows,
  pagination,
  rowPinning,
  sorting,
  type FacetsSchema,
} from "@lindaflor/shared/lib/utils";
import { z } from "zod";

export const rowSchema = z.object({
  id: z.guid(),
  name: z.string(),
  state: z.string(),
  organization_id: z.guid(),
  created_by_user_id: z.guid(),
  created_at: z.date(),
  updated_at: z.date(),
});
export type ConcessionOutput = z.infer<typeof rowSchema>;

export const STATE_VALUES = [
  "Acre",
  "Alagoas",
  "Amapá",
  "Amazonas",
  "Bahia",
  "Ceará",
  "Distrito Federal",
  "Espírito Santo",
  "Goiás",
  "Maranhão",
  "Mato Grosso",
  "Mato Grosso do Sul",
  "Minas Gerais",
  "Pará",
  "Paraíba",
  "Paraná",
  "Pernambuco",
  "Piauí",
  "Rio de Janeiro",
  "Rio Grande do Norte",
  "Rio Grande do Sul",
  "Rondônia",
  "Roraima",
  "Santa Catarina",
  "São Paulo",
  "Sergipe",
  "Tocantins",
] as const;

export const stateOptions = STATE_VALUES.map((value) => ({
  value,
  label: value,
}));

export const stateFilterValue = z.array(z.enum(STATE_VALUES)).min(1);

export const defaultFacets: FacetsSchema = {
  state: {
    type: "select",
    label: "State",
    options: stateOptions.map((s) => ({
      value: s.value,
      label: s.label,
      count: 0,
    })),
  },
  created_at: { type: "date", label: "Created at" },
  updated_at: { type: "date", label: "Updated at" },
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
          .min(1, { message: "O nome da concessão é obrigatório" })
          .trim(),
        state: z.enum(STATE_VALUES, { message: "Estado selecionado inválido" }),
      }),
      output: rowSchema,
    },

    update: {
      input: z.object({
        id: z.guid(),
        name: z
          .string()
          .min(1, { message: "O nome da concessão é obrigatório" })
          .trim()
          .optional(),
        state: z
          .enum(STATE_VALUES, { message: "Estado selecionado inválido" })
          .optional(),
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
