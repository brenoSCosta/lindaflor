import { labels, priorities, statuses } from "@lindaflor/shared/enums/todo";
import {
  columnFilters,
  facetsSchema,
  globalFilter,
  keepPinnedRows,
  pagination,
  rowPinning,
  rowSelection,
  sorting,
  type DateFacetMeta,
  type SelectFacetMeta,
  type ToolbarFacetOption,
} from "@lindaflor/shared/lib/utils";
import { z } from "zod";

const MAX_TODO_LENGTH = 500;

export const statusFilterValue = z.array(z.enum(statuses)).min(1);
export const labelFilterValue = z.array(z.enum(labels)).min(1);
export const priorityFilterValue = z.array(z.enum(priorities)).min(1);

const data = z.object({
  id: z.guid(),
  text: z.string(),
  status: z.enum(statuses),
  label: z.enum(labels),
  priority: z.enum(priorities),
  estimated_hours: z.number().nullable(),
  actual_hours: z.number().nullable(),
  progress: z.number().nullable(),
  cost: z.number().nullable(),
  due_date: z.date().nullable(),
  completed_at: z.date().nullable(),
  created_at: z.date(),
  updated_at: z.date(),
  organization_id: z.guid(),
});
export type TodoOutput = z.infer<typeof data>;

export const statusOptions: ToolbarFacetOption[] = [
  { value: "backlog", label: "Backlog", icon: "HelpCircle" },
  { value: "todo", label: "Todo", icon: "Circle" },
  { value: "in progress", label: "In Progress", icon: "Timer" },
  { value: "done", label: "Done", icon: "CheckCircle" },
  { value: "canceled", label: "Canceled", icon: "CircleOff" },
];

export const labelOptions: ToolbarFacetOption[] = [
  { value: "bug", label: "Bug", icon: "Bug" },
  { value: "feature", label: "Feature", icon: "Flag" },
  { value: "documentation", label: "Documentation", icon: "Book" },
];

export const priorityOptions: ToolbarFacetOption[] = [
  { value: "low", label: "Low", icon: "ArrowDown" },
  { value: "medium", label: "Medium", icon: "ArrowRight" },
  { value: "high", label: "High", icon: "ArrowUp" },
];

export type DefaultFacetsShape = {
  status: SelectFacetMeta;
  label: SelectFacetMeta;
  priority: SelectFacetMeta;
  created_at: DateFacetMeta;
  updated_at: DateFacetMeta;
  completed_at: DateFacetMeta;
};

export const defaultFacets: DefaultFacetsShape = {
  status: { type: "select", label: "Status", options: statusOptions },
  priority: { type: "select", label: "Priority", options: priorityOptions },
  label: { type: "select", label: "Label", options: labelOptions },
  created_at: { type: "date", label: "Created at" },
  updated_at: { type: "date", label: "Updated at" },
  completed_at: { type: "date", label: "Completed at" },
};

export const schema = {
  v2: {
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
        data: z.array(data),
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
      output: data,
    },

    create: {
      input: z.object({
        text: z
          .string()
          .min(1, { error: "Todo text is required" })
          .max(MAX_TODO_LENGTH, {
            error: `Todo text must be less than ${MAX_TODO_LENGTH} characters`,
          })
          .trim(),
        status: z.enum(statuses).default("todo"),
        label: z.enum(labels).default("documentation"),
        priority: z.enum(priorities).default("medium"),
        estimated_hours: z.number().optional(),
        actual_hours: z.number().optional(),
        progress: z.number().optional(),
        cost: z.number().optional(),
        due_date: z.date().optional(),
        completed_at: z.date().optional(),
      }),
      output: data,
    },

    update: {
      input: z.object({
        id: z.guid(),
        text: z
          .string()
          .min(1, { error: "Todo text is required" })
          .max(MAX_TODO_LENGTH, {
            error: `Todo text must be less than ${MAX_TODO_LENGTH} characters`,
          })
          .trim()
          .optional(),
        status: z.enum(statuses).optional(),
        label: z.enum(labels).optional(),
        priority: z.enum(priorities).optional(),
        estimated_hours: z.number().optional(),
        actual_hours: z.number().optional(),
        progress: z.number().optional(),
        cost: z.number().optional(),
        due_date: z.date().optional(),
        completed_at: z.date().optional(),
      }),
      output: data,
    },

    delete: {
      input: z.object({
        ids: z.array(z.guid()).min(1),
      }),
      output: z.null(),
    },

    getSelected: {
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
      output: z.array(data),
    },
  },
};
