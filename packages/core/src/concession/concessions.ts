import { db } from "@lindaflor/db";
import { concessions } from "@lindaflor/db/schema/tankage";
import {
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import type { FacetsSchema } from "@lindaflor/shared/lib/utils";
import {
  defaultFacets,
  schema,
  stateFilterValue,
} from "@lindaflor/shared/schemas/concession";
import { ORPCError } from "@orpc/server";
import {
  and,
  asc,
  desc,
  eq,
  ilike,
  inArray,
  like,
  or,
  sql,
  type SQL,
} from "drizzle-orm";
import type { z } from "zod";

type GetAllConcessionsInput = z.infer<typeof schema.v1.getAll.input>;

export async function getAllConcessions(params: {
  input: GetAllConcessionsInput | undefined;
  organizationId: string;
}) {
  const { input, organizationId } = params;
  const baseFilter = eq(concessions.organization_id, organizationId);

  if (!input) {
    const data = await db
      .select()
      .from(concessions)
      .where(baseFilter)
      .orderBy(asc(concessions.created_at));
    return { data: schema.v1.getAll.output.shape.data.parse(data) };
  }

  const { pageIndex, pageSize } = input.pagination ?? {
    pageIndex: 0,
    pageSize: 10,
  };
  const sorting = input.sorting ?? [];
  const columnFilters = input.columnFilters ?? [];
  const globalFilter = input.globalFilter;

  const globalFilterCondition = globalFilter
    ? or(
        ilike(concessions.name, `%${globalFilter}%`),
        ilike(concessions.state, `%${globalFilter}%`),
      )
    : undefined;

  const dynamicFilters = columnFilters.flatMap((f) => {
    const condition: SQL | undefined = (() => {
      switch (f.id) {
        case "name":
          if (typeof f.value !== "string") return undefined;
          return like(concessions.name, `%${f.value}%`);
        case "state": {
          const parsed = stateFilterValue.safeParse(f.value);
          if (!parsed.success) return undefined;
          return inArray(concessions.state, parsed.data);
        }
        default:
          return undefined;
      }
    })();
    return condition ? [condition] : [];
  });

  const whereClause = and(baseFilter, globalFilterCondition, ...dynamicFilters);

  const orderByClause = sorting
    .map((s) => {
      switch (s.id) {
        case "name":
          return s.desc ? desc(concessions.name) : asc(concessions.name);
        case "state":
          return s.desc ? desc(concessions.state) : asc(concessions.state);
        case "created_at":
          return s.desc
            ? desc(concessions.created_at)
            : asc(concessions.created_at);
        case "updated_at":
          return s.desc
            ? desc(concessions.updated_at)
            : asc(concessions.updated_at);
        default:
          return undefined;
      }
    })
    .filter((x): x is ReturnType<typeof asc> => x !== undefined);

  const effectiveOrderBy =
    orderByClause.length > 0 ? orderByClause : [asc(concessions.created_at)];

  const [data, rowCount, stateRows] = await Promise.all([
    db
      .select()
      .from(concessions)
      .where(whereClause)
      .orderBy(...effectiveOrderBy)
      .limit(pageSize)
      .offset(pageIndex * pageSize),
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(concessions)
      .where(whereClause)
      .then((rows) => rows[0]?.count ?? 0),
    db
      .select({
        val: concessions.state,
        count: sql<number>`count(*)::int`,
      })
      .from(concessions)
      .where(baseFilter)
      .groupBy(concessions.state),
  ]);

  const stateCountMap = new Map(stateRows.map((r) => [r.val, r.count]));
  const stateFacet = defaultFacets.state;
  const stateOptions =
    stateFacet?.type === "select"
      ? stateFacet.options.map((s) => ({
          value: s.value,
          label: s.label,
          count: stateCountMap.get(s.value) ?? 0,
        }))
      : [];
  const facets: FacetsSchema = {
    state: {
      type: "select",
      label: "State",
      options: stateOptions,
    },
    created_at: defaultFacets.created_at ?? {
      type: "date",
      label: "Created at",
    },
    updated_at: defaultFacets.updated_at ?? {
      type: "date",
      label: "Updated at",
    },
  };

  return schema.v1.getAll.output.parse({
    data,
    meta: { rowCount, facets },
  });
}

type GetConcessionByIdInput = z.infer<typeof schema.v1.getById.input>;

export async function getConcessionById(params: {
  input: GetConcessionByIdInput;
  ability: AppAbility;
}) {
  const [row] = await db
    .select()
    .from(concessions)
    .where(eq(concessions.id, params.input.id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Concessão não encontrada",
    });
  }
  if (params.ability.cannot("read", subject("Concessions", row))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler esta concessão",
    });
  }
  return schema.v1.getById.output.parse(row);
}

type CreateConcessionInput = z.infer<typeof schema.v1.create.input>;

export async function createConcession(params: {
  input: CreateConcessionInput;
  organizationId: string;
  userId: string;
}) {
  const [created] = await db
    .insert(concessions)
    .values({
      ...params.input,
      organization_id: params.organizationId,
      created_by_user_id: params.userId,
    })
    .returning();
  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar concessão",
    });
  }
  return schema.v1.create.output.parse(created);
}

type UpdateConcessionInput = z.infer<typeof schema.v1.update.input>;

export async function updateConcession(params: {
  input: UpdateConcessionInput;
  ability: AppAbility;
}) {
  const { id, ...data } = params.input;
  const [existing] = await db
    .select()
    .from(concessions)
    .where(eq(concessions.id, id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Concessão não encontrada",
    });
  }
  if (params.ability.cannot("update", subject("Concessions", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para atualizar esta concessão",
    });
  }
  await db.update(concessions).set(data).where(eq(concessions.id, id));
  const [updated] = await db
    .select()
    .from(concessions)
    .where(eq(concessions.id, id));
  if (!updated) {
    throw new ORPCError("NOT_FOUND", {
      message: "Concessão não encontrada",
    });
  }
  return schema.v1.update.output.parse(updated);
}

type DeleteConcessionsInput = z.infer<typeof schema.v1.delete.input>;

export async function deleteConcessions(params: {
  input: DeleteConcessionsInput;
  ability: AppAbility;
}) {
  const rows = await db
    .select()
    .from(concessions)
    .where(inArray(concessions.id, params.input.ids));
  if (rows.length === 0) {
    throw new ORPCError("NOT_FOUND", {
      message: "Nenhuma concessão encontrada",
    });
  }
  const notFoundIds = params.input.ids.filter(
    (id) => !rows.some((r) => r.id === id),
  );
  if (notFoundIds.length > 0) {
    throw new ORPCError("NOT_FOUND", {
      message: `Concessions not found: ${notFoundIds.join(", ")}`,
    });
  }
  const unauthorized = rows.filter((r) =>
    params.ability.cannot("delete", subject("Concessions", r)),
  );
  if (unauthorized.length > 0) {
    throw new ORPCError("FORBIDDEN", {
      message:
        "You do not have permission to delete one or more of these concessions",
    });
  }
  await db.delete(concessions).where(inArray(concessions.id, params.input.ids));
  return null;
}
