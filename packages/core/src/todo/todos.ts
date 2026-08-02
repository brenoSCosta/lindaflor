import { dateFilterToCondition } from "@lindaflor/core/lib/date-filter";
import { db } from "@lindaflor/db";
import { todo } from "@lindaflor/db/schema/todo";
import { labels, priorities, statuses } from "@lindaflor/shared/enums/todo";
import {
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import { parseDateFilterValue } from "@lindaflor/shared/lib/date-filter";
import type {
  ColumnFilters,
  FacetsSchema,
  GlobalFilter,
  RowSelection,
  Sorting,
} from "@lindaflor/shared/lib/utils";
import {
  defaultFacets,
  labelFilterValue,
  priorityFilterValue,
  schema,
  statusFilterValue,
} from "@lindaflor/shared/schemas/todo";
import { ORPCError } from "@orpc/server";
import {
  and,
  asc,
  desc,
  eq,
  ilike,
  inArray,
  like,
  not,
  or,
  sql,
  type SQL,
} from "drizzle-orm";
import type { z } from "zod";

type GetAllTodosInput = z.infer<typeof schema.v2.getAll.input>;

export async function getAllTodos(params: {
  input: GetAllTodosInput | undefined;
  organizationId: string;
  clientTimezone: string;
}) {
  const { input, organizationId, clientTimezone } = params;
  const baseFilter = eq(todo.organization_id, organizationId);

  if (!input) {
    const data = await db
      .select()
      .from(todo)
      .where(baseFilter)
      .orderBy(asc(todo.created_at));
    return { data: schema.v2.getAll.output.shape.data.parse(data) };
  }

  const { pageIndex, pageSize } = input.pagination ?? {
    pageIndex: 0,
    pageSize: 10,
  };
  const sorting = input.sorting ?? [];
  const columnFilters = input.columnFilters ?? [];
  const globalFilter = input.globalFilter;
  const pinnedTop = input?.rowPinning?.top ?? [];
  const pinnedBottom = input?.rowPinning?.bottom ?? [];
  const keepPinning = input?.keepPinnedRows ?? true;
  const hasPinning = pinnedTop.length + pinnedBottom.length > 0;

  const whereClause = buildWhereClause({
    organizationId,
    columnFilters,
    globalFilter,
    clientTimezone,
  });

  const effectiveOrderBy = buildOrderBy(sorting);

  const [mainRows, rowCount, facetsResult] = await Promise.all([
    db
      .select()
      .from(todo)
      .where(whereClause)
      .orderBy(...effectiveOrderBy)
      .limit(pageSize)
      .offset(pageIndex * pageSize),
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(todo)
      .where(whereClause)
      .then((rows) => rows[0]?.count ?? 0),
    Promise.all([
      db
        .select({ val: todo.status, count: sql<number>`count(*)::int` })
        .from(todo)
        .where(baseFilter)
        .groupBy(todo.status),
      db
        .select({
          val: todo.priority,
          count: sql<number>`count(*)::int`,
        })
        .from(todo)
        .where(baseFilter)
        .groupBy(todo.priority),
      db
        .select({ val: todo.label, count: sql<number>`count(*)::int` })
        .from(todo)
        .where(baseFilter)
        .groupBy(todo.label),
    ]),
  ]);

  const todos =
    hasPinning && keepPinning
      ? await (async () => {
          const fetchedIds = new Set(mainRows.map((r) => r.id));
          const missingTop = pinnedTop.filter((id) => !fetchedIds.has(id));
          const missingBottom = pinnedBottom.filter(
            (id) => !fetchedIds.has(id),
          );

          const [pinnedTopRows, pinnedBottomRows] = await Promise.all([
            missingTop.length > 0
              ? db
                  .select()
                  .from(todo)
                  .where(and(baseFilter, inArray(todo.id, missingTop)))
              : Promise.resolve([]),
            missingBottom.length > 0
              ? db
                  .select()
                  .from(todo)
                  .where(and(baseFilter, inArray(todo.id, missingBottom)))
              : Promise.resolve([]),
          ]);

          return [...pinnedTopRows, ...mainRows, ...pinnedBottomRows];
        })()
      : mainRows;

  const [statusRows, priorityRows, labelRows] = facetsResult;
  const statusCountMap = new Map(statusRows.map((r) => [r.val, r.count]));
  const priorityCountMap = new Map(priorityRows.map((r) => [r.val, r.count]));
  const labelCountMap = new Map(labelRows.map((r) => [r.val, r.count]));

  const facets: FacetsSchema = {
    status: {
      type: "select",
      label: defaultFacets.status.label,
      options: statuses.map((s) => ({
        value: s,
        count: statusCountMap.get(s) ?? 0,
        label: defaultFacets.status.options.find((option) => option.value === s)
          ?.label,
        icon: defaultFacets.status.options.find((option) => option.value === s)
          ?.icon,
      })),
    },
    priority: {
      type: "select",
      label: defaultFacets.priority.label,
      options: priorities.map((p) => ({
        value: p,
        count: priorityCountMap.get(p) ?? 0,
        label: defaultFacets.priority.options.find(
          (option) => option.value === p,
        )?.label,
        icon: defaultFacets.priority.options.find(
          (option) => option.value === p,
        )?.icon,
      })),
    },
    label: {
      type: "select",
      label: defaultFacets.label.label,
      options: labels.map((l) => ({
        value: l,
        count: labelCountMap.get(l) ?? 0,
        label: defaultFacets.label.options.find((option) => option.value === l)
          ?.label,
        icon: defaultFacets.label.options.find((option) => option.value === l)
          ?.icon,
      })),
    },
    created_at: defaultFacets.created_at,
    updated_at: defaultFacets.updated_at,
    completed_at: defaultFacets.completed_at,
  };

  const result = {
    data: todos,
    meta: {
      rowCount,
      facets,
    },
  };

  return schema.v2.getAll.output.parse(result);
}

type GetTodoByIdInput = z.infer<typeof schema.v2.getById.input>;

export async function getTodoById(params: {
  input: GetTodoByIdInput;
  ability: AppAbility;
}) {
  const [todoById] = await db
    .select()
    .from(todo)
    .where(eq(todo.id, params.input.id));

  if (!todoById) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tarefa não encontrada",
    });
  }
  if (params.ability.cannot("read", subject("Todo", todoById))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler esta tarefa",
    });
  }
  return schema.v2.getById.output.parse(todoById);
}

type CreateTodoInput = z.infer<typeof schema.v2.create.input>;

export async function createTodo(params: {
  input: CreateTodoInput;
  organizationId: string;
}) {
  const [createdTodo] = await db
    .insert(todo)
    .values({
      ...params.input,
      organization_id: params.organizationId,
    })
    .returning();
  if (!createdTodo) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar tarefa",
    });
  }
  return schema.v2.create.output.parse(createdTodo);
}

type UpdateTodoInput = z.infer<typeof schema.v2.update.input>;

export async function updateTodo(params: {
  input: UpdateTodoInput;
  ability: AppAbility;
}) {
  const { id, ...data } = params.input;
  const [existingTodo] = await db.select().from(todo).where(eq(todo.id, id));
  if (!existingTodo) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tarefa não encontrada",
    });
  }
  if (params.ability.cannot("update", subject("Todo", existingTodo))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para atualizar esta tarefa",
    });
  }
  await db.update(todo).set(data).where(eq(todo.id, id));
  const [updatedTodo] = await db.select().from(todo).where(eq(todo.id, id));
  if (!updatedTodo) {
    throw new ORPCError("NOT_FOUND", {
      message: "Tarefa não encontrada",
    });
  }
  return schema.v2.update.output.parse(updatedTodo);
}

type DeleteTodosInput = z.infer<typeof schema.v2.delete.input>;

export async function deleteTodos(params: {
  input: DeleteTodosInput;
  ability: AppAbility;
}) {
  const todosToDelete = await db
    .select()
    .from(todo)
    .where(inArray(todo.id, params.input.ids));

  if (todosToDelete.length === 0) {
    throw new ORPCError("NOT_FOUND", {
      message: "Nenhuma tarefa encontrada",
    });
  }

  const notFoundIds = params.input.ids.filter(
    (id) => !todosToDelete.some((t) => t.id === id),
  );
  if (notFoundIds.length > 0) {
    throw new ORPCError("NOT_FOUND", {
      message: `Todos not found: ${notFoundIds.join(", ")}`,
    });
  }

  const unauthorized = todosToDelete.filter((t) =>
    params.ability.cannot("delete", subject("Todo", t)),
  );
  if (unauthorized.length > 0) {
    throw new ORPCError("FORBIDDEN", {
      message:
        "You do not have permission to delete one or more of these todos",
    });
  }

  await db.delete(todo).where(inArray(todo.id, params.input.ids));
  return null;
}

type GetSelectedTodosInput = z.infer<typeof schema.v2.getSelected.input>;

export async function getSelectedTodos(params: {
  input: GetSelectedTodosInput;
  organizationId: string;
  clientTimezone: string;
}) {
  const { input, organizationId, clientTimezone } = params;
  const isSelectAll = input.all === true;

  const explicitIds: string[] = isSelectAll
    ? []
    : input.selection
      ? extractExplicitIds(input.selection)
      : [];

  if (!isSelectAll && explicitIds.length === 0) {
    return [];
  }

  const excludeIds = input.exclude ?? [];
  const whereClause = isSelectAll
    ? buildWhereClause({
        organizationId,
        columnFilters: input.columnFilters,
        globalFilter: input.globalFilter,
        clientTimezone,
        excludeIds,
      })
    : and(
        eq(todo.organization_id, organizationId),
        inArray(todo.id, explicitIds),
      );

  const effectiveOrderBy = isSelectAll
    ? buildOrderBy(input.sorting ?? [])
    : [asc(todo.id)];

  const rows = await db
    .select()
    .from(todo)
    .where(whereClause)
    .orderBy(...effectiveOrderBy);

  return schema.v2.getSelected.output.parse(rows);
}

function extractExplicitIds(selection: RowSelection): string[] {
  const ids: string[] = [];
  for (const [id, selected] of Object.entries(selection)) {
    if (selected) {
      ids.push(id);
    }
  }
  return ids;
}

interface WhereClauseParams {
  organizationId: string;
  columnFilters?: ColumnFilters;
  globalFilter?: GlobalFilter;
  clientTimezone: string;
  excludeIds?: string[];
}

function buildWhereClause({
  organizationId,
  columnFilters = [],
  globalFilter,
  clientTimezone,
  excludeIds = [],
}: WhereClauseParams): SQL | undefined {
  const baseFilter = eq(todo.organization_id, organizationId);

  const excludeCondition =
    excludeIds.length > 0 ? not(inArray(todo.id, excludeIds)) : undefined;

  const globalFilterCondition = globalFilter
    ? or(
        ilike(todo.text, `%${globalFilter}%`),
        ilike(sql`${todo.status}::text`, `%${globalFilter}%`),
        ilike(sql`${todo.label}::text`, `%${globalFilter}%`),
        ilike(sql`${todo.priority}::text`, `%${globalFilter}%`),
      )
    : undefined;

  const dynamicFilters = columnFilters.flatMap((f) => {
    const condition: SQL | undefined = (() => {
      switch (f.id) {
        case "text":
          if (typeof f.value !== "string") {
            return undefined;
          }
          return like(todo.text, `%${f.value}%`);
        case "status": {
          const parsed = statusFilterValue.safeParse(f.value);
          if (!parsed.success) return undefined;
          return inArray(todo.status, parsed.data);
        }
        case "label": {
          const parsed = labelFilterValue.safeParse(f.value);
          if (!parsed.success) return undefined;
          return inArray(todo.label, parsed.data);
        }
        case "priority": {
          const parsed = priorityFilterValue.safeParse(f.value);
          if (!parsed.success) return undefined;
          return inArray(todo.priority, parsed.data);
        }
        case "created_at": {
          const parsed = parseDateFilterValue(f.value);
          if (!parsed) return undefined;
          const ranges = Array.isArray(parsed) ? parsed : [parsed];
          const conditions = ranges
            .map((p) =>
              dateFilterToCondition(todo.created_at, p, {
                clientTimezone,
              }),
            )
            .filter((c): c is SQL => c !== undefined);
          return conditions.length === 0
            ? undefined
            : conditions.length === 1
              ? conditions[0]
              : or(...conditions);
        }
        case "updated_at": {
          const parsed = parseDateFilterValue(f.value);
          if (!parsed) return undefined;
          const ranges = Array.isArray(parsed) ? parsed : [parsed];
          const conditions = ranges
            .map((p) =>
              dateFilterToCondition(todo.updated_at, p, {
                clientTimezone,
              }),
            )
            .filter((c): c is SQL => c !== undefined);
          return conditions.length === 0
            ? undefined
            : conditions.length === 1
              ? conditions[0]
              : or(...conditions);
        }
        case "completed_at": {
          const parsed = parseDateFilterValue(f.value);
          if (!parsed) return undefined;
          const ranges = Array.isArray(parsed) ? parsed : [parsed];
          const conditions = ranges
            .map((p) =>
              dateFilterToCondition(todo.completed_at, p, {
                clientTimezone,
              }),
            )
            .filter((c): c is SQL => c !== undefined);
          return conditions.length === 0
            ? undefined
            : conditions.length === 1
              ? conditions[0]
              : or(...conditions);
        }
        default:
          return undefined;
      }
    })();
    return condition ? [condition] : [];
  });

  return and(
    baseFilter,
    excludeCondition,
    globalFilterCondition,
    ...dynamicFilters,
  );
}

function buildOrderBy(sorting: Sorting): ReturnType<typeof asc>[] {
  const orderByClause = sorting
    .map((s) => {
      switch (s.id) {
        case "text":
          return s.desc ? desc(todo.text) : asc(todo.text);
        case "status":
          return s.desc ? desc(todo.status) : asc(todo.status);
        case "label":
          return s.desc ? desc(todo.label) : asc(todo.label);
        case "priority":
          return s.desc ? desc(todo.priority) : asc(todo.priority);
        case "created_at":
          return s.desc ? desc(todo.id) : asc(todo.id);
        case "updated_at":
          return s.desc ? desc(todo.updated_at) : asc(todo.updated_at);
        default:
          return undefined;
      }
    })
    .filter((x): x is ReturnType<typeof asc> => x !== undefined);

  return orderByClause.length > 0 ? orderByClause : [asc(todo.id)];
}
