import { FACET_ICON_NAMES } from "@lindaflor/shared/lib/facet-icons";
import type {
  ColumnFiltersState,
  PaginationOptions,
  PaginationState,
  RowPinningState,
  RowSelectionState,
  SortingState,
} from "@tanstack/react-table";
import { z } from "zod";

export type { FacetIconName } from "@lindaflor/shared/lib/facet-icons";

export const sorting: z.ZodType<SortingState> = z.array(
  z.object({
    id: z.string(),
    desc: z.boolean(),
  }),
);
export type Sorting = z.infer<typeof sorting>;

export const pagination: z.ZodType<PaginationState> = z.object({
  pageIndex: z.number(),
  pageSize: z.number(),
});
export type Pagination = z.infer<typeof pagination>;

export const rowCount: z.ZodType<Required<PaginationOptions>["rowCount"]> =
  z.number();
export type RowCount = z.infer<typeof rowCount>;

export const columnFilters: z.ZodType<ColumnFiltersState> = z.array(
  z.object({
    id: z.string(),
    value: z.unknown(),
  }),
);
export type ColumnFilters = z.infer<typeof columnFilters>;

export const toolbarFacetOption = z.object({
  value: z.string(),
  label: z.string().optional(),
  icon: z.enum(FACET_ICON_NAMES).optional(),
  count: z.number().optional(),
});
export type ToolbarFacetOption = z.infer<typeof toolbarFacetOption>;

export const selectFacetMeta = z.object({
  type: z.literal("select"),
  label: z.string().optional(),
  options: z.array(toolbarFacetOption),
});
export type SelectFacetMeta = z.infer<typeof selectFacetMeta>;

export const dateFacetMeta = z.object({
  type: z.literal("date"),
  label: z.string(),
});
export type DateFacetMeta = z.infer<typeof dateFacetMeta>;

export const facetMetaUnion = z.discriminatedUnion("type", [
  selectFacetMeta,
  dateFacetMeta,
]);
export type FacetMeta = z.infer<typeof facetMetaUnion>;
export type ToolbarFacets = Record<string, FacetMeta>;

export const facetsSchema = z.record(z.string(), facetMetaUnion);
export type FacetsSchema = z.infer<typeof facetsSchema>;

export const globalFilter = z.string().optional();
export type GlobalFilter = z.infer<typeof globalFilter>;

export const rowPinning: z.ZodType<RowPinningState> = z.object({
  top: z.array(z.string()).optional(),
  bottom: z.array(z.string()).optional(),
});
export type RowPinning = z.infer<typeof rowPinning>;

export const keepPinnedRows = z.boolean().optional();
export type KeepPinnedRows = z.infer<typeof keepPinnedRows>;

export const rowSelection: z.ZodType<RowSelectionState> = z.record(
  z.string(),
  z.boolean(),
);
export type RowSelection = z.infer<typeof rowSelection>;
