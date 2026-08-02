import { db } from "@lindaflor/db";
import { careers } from "@lindaflor/db/schema/career";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/career";
import { ORPCError } from "@orpc/server";
import { desc, eq } from "drizzle-orm";
import type { z } from "zod";

type ListCareersInput = z.infer<typeof schema.v1.list.input>;

export async function listCareers(params: { input: ListCareersInput }) {
  const data = await db
    .select()
    .from(careers)
    .where(params.input.onlyActive ? eq(careers.is_active, true) : undefined)
    .orderBy(desc(careers.created_at));

  return schema.v1.list.output.parse({ data });
}

type CreateCareerInput = z.infer<typeof schema.v1.create.input>;

export async function createCareer(params: { input: CreateCareerInput }) {
  const [created] = await db
    .insert(careers)
    .values({
      title: params.input.title,
      department: params.input.department,
      location: params.input.location,
      type: params.input.type,
      description: params.input.description ?? null,
      requirements: params.input.requirements,
      is_active: params.input.is_active ?? true,
    })
    .returning();

  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar vaga",
    });
  }

  return schema.v1.create.output.parse(created);
}

type UpdateCareerInput = z.infer<typeof schema.v1.update.input>;

export async function updateCareer(params: {
  input: UpdateCareerInput;
  ability: AppAbility;
}) {
  const { id, ...data } = params.input;

  const [existing] = await db.select().from(careers).where(eq(careers.id, id));

  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Vaga não encontrada",
    });
  }

  if (!params.ability.can("update", "Curriculum")) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para atualizar esta vaga",
    });
  }

  await db.update(careers).set(data).where(eq(careers.id, id));

  const [updated] = await db.select().from(careers).where(eq(careers.id, id));

  if (!updated) {
    throw new ORPCError("NOT_FOUND", {
      message: "Vaga não encontrada",
    });
  }

  return schema.v1.update.output.parse(updated);
}

type DeleteCareerInput = z.infer<typeof schema.v1.delete.input>;

export async function deleteCareer(params: {
  input: DeleteCareerInput;
  ability: AppAbility;
}) {
  const [existing] = await db
    .select()
    .from(careers)
    .where(eq(careers.id, params.input.id));

  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Vaga não encontrada",
    });
  }

  if (!params.ability.can("delete", "Curriculum")) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para excluir esta vaga",
    });
  }

  await db.delete(careers).where(eq(careers.id, params.input.id));
  return null;
}
