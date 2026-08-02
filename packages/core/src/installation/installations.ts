import { db } from "@lindaflor/db";
import { installations } from "@lindaflor/db/schema/tankage";
import {
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import type { FacetsSchema } from "@lindaflor/shared/lib/utils";
import { defaultFacets, schema } from "@lindaflor/shared/schemas/installation";
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

type GetAllInstallationsInput = z.infer<typeof schema.v1.getAll.input>;

export async function getAllInstallations(params: {
  input: GetAllInstallationsInput | undefined;
  organizationId: string;
}) {
  const { input, organizationId } = params;
  const baseFilter = eq(installations.organization_id, organizationId);

  if (!input) {
    const data = await db
      .select()
      .from(installations)
      .where(baseFilter)
      .orderBy(asc(installations.created_at));
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
        ilike(installations.name, `%${globalFilter}%`),
        ilike(installations.concession_id, `%${globalFilter}%`),
      )
    : undefined;

  const dynamicFilters = columnFilters.flatMap((f) => {
    const condition: SQL | undefined = (() => {
      switch (f.id) {
        case "name":
          if (typeof f.value !== "string") return undefined;
          return like(installations.name, `%${f.value}%`);
        case "concession_id":
          if (typeof f.value !== "string") return undefined;
          return eq(installations.concession_id, f.value);
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
          return s.desc ? desc(installations.name) : asc(installations.name);
        case "created_at":
          return s.desc
            ? desc(installations.created_at)
            : asc(installations.created_at);
        case "updated_at":
          return s.desc
            ? desc(installations.updated_at)
            : asc(installations.updated_at);
        default:
          return undefined;
      }
    })
    .filter((x): x is ReturnType<typeof asc> => x !== undefined);

  const effectiveOrderBy =
    orderByClause.length > 0 ? orderByClause : [asc(installations.created_at)];

  const [data, rowCount] = await Promise.all([
    db
      .select()
      .from(installations)
      .where(whereClause)
      .orderBy(...effectiveOrderBy)
      .limit(pageSize)
      .offset(pageIndex * pageSize),
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(installations)
      .where(whereClause)
      .then((rows) => rows[0]?.count ?? 0),
  ]);

  const facets: FacetsSchema = {
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

type GetInstallationByIdInput = z.infer<typeof schema.v1.getById.input>;

export async function getInstallationById(params: {
  input: GetInstallationByIdInput;
  ability: AppAbility;
}) {
  const [row] = await db
    .select()
    .from(installations)
    .where(eq(installations.id, params.input.id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Instalação não encontrada",
    });
  }
  if (params.ability.cannot("read", subject("Installations", row))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler esta instalação",
    });
  }
  return schema.v1.getById.output.parse(row);
}

type CreateInstallationInput = z.infer<typeof schema.v1.create.input>;

export async function createInstallation(params: {
  input: CreateInstallationInput;
  organizationId: string;
  userId: string;
}) {
  const [created] = await db
    .insert(installations)
    .values({
      ...params.input,
      organization_id: params.organizationId,
      created_by_user_id: params.userId,
    })
    .returning();
  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar instalação",
    });
  }
  return schema.v1.create.output.parse(created);
}

type UpdateInstallationInput = z.infer<typeof schema.v1.update.input>;

export async function updateInstallation(params: {
  input: UpdateInstallationInput;
  ability: AppAbility;
}) {
  const { id, ...data } = params.input;
  const [existing] = await db
    .select()
    .from(installations)
    .where(eq(installations.id, id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Instalação não encontrada",
    });
  }
  if (params.ability.cannot("update", subject("Installations", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para atualizar esta instalação",
    });
  }
  await db.update(installations).set(data).where(eq(installations.id, id));
  const [updated] = await db
    .select()
    .from(installations)
    .where(eq(installations.id, id));
  if (!updated) {
    throw new ORPCError("NOT_FOUND", {
      message: "Instalação não encontrada",
    });
  }
  return schema.v1.update.output.parse(updated);
}

type DeleteInstallationsInput = z.infer<typeof schema.v1.delete.input>;

export async function deleteInstallations(params: {
  input: DeleteInstallationsInput;
  ability: AppAbility;
}) {
  const rows = await db
    .select()
    .from(installations)
    .where(inArray(installations.id, params.input.ids));
  if (rows.length === 0) {
    throw new ORPCError("NOT_FOUND", {
      message: "Nenhuma instalação encontrada",
    });
  }
  const notFoundIds = params.input.ids.filter(
    (id) => !rows.some((r) => r.id === id),
  );
  if (notFoundIds.length > 0) {
    throw new ORPCError("NOT_FOUND", {
      message: `Installations not found: ${notFoundIds.join(", ")}`,
    });
  }
  const unauthorized = rows.filter((r) =>
    params.ability.cannot("delete", subject("Installations", r)),
  );
  if (unauthorized.length > 0) {
    throw new ORPCError("FORBIDDEN", {
      message:
        "You do not have permission to delete one or more of these installations",
    });
  }
  await db
    .delete(installations)
    .where(inArray(installations.id, params.input.ids));
  return null;
}
