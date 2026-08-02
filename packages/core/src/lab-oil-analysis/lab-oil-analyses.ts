import { db } from "@lindaflor/db";
import { lab_oil_analyses, tanks } from "@lindaflor/db/schema/tankage";
import {
  subject,
  type AppAbility,
} from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/lab-oil-analysis";
import { ORPCError } from "@orpc/server";
import { and, desc, eq } from "drizzle-orm";
import type { z } from "zod";

async function requireTankInOrg(tankId: string, organizationId: string) {
  const [tank] = await db
    .select({ id: tanks.id })
    .from(tanks)
    .where(
      and(eq(tanks.id, tankId), eq(tanks.organization_id, organizationId)),
    );
  if (!tank) {
    throw new ORPCError("NOT_FOUND", { message: "Tanque não encontrado" });
  }
  return tank;
}

type ListLabOilAnalysesByTankInput = z.infer<typeof schema.v1.listByTank.input>;

export async function listLabOilAnalysesByTank(params: {
  input: ListLabOilAnalysesByTankInput;
  organizationId: string;
}) {
  const { input, organizationId } = params;
  await requireTankInOrg(input.tank_id, organizationId);

  const data = await db
    .select()
    .from(lab_oil_analyses)
    .where(
      and(
        eq(lab_oil_analyses.tank_id, input.tank_id),
        eq(lab_oil_analyses.organization_id, organizationId),
      ),
    )
    .orderBy(desc(lab_oil_analyses.collected_at));

  return schema.v1.listByTank.output.parse({ data });
}

type GetLabOilAnalysisByIdInput = z.infer<typeof schema.v1.getById.input>;

export async function getLabOilAnalysisById(params: {
  input: GetLabOilAnalysisByIdInput;
  ability: AppAbility;
}) {
  const [row] = await db
    .select()
    .from(lab_oil_analyses)
    .where(eq(lab_oil_analyses.id, params.input.id));
  if (!row) {
    throw new ORPCError("NOT_FOUND", {
      message: "Análise de laboratório não encontrada",
    });
  }
  if (params.ability.cannot("read", subject("LabOilAnalyses", row))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler esta análise de laboratório",
    });
  }
  return schema.v1.getById.output.parse(row);
}

type CreateLabOilAnalysisInput = z.infer<typeof schema.v1.create.input>;

export async function createLabOilAnalysis(params: {
  input: CreateLabOilAnalysisInput;
  organizationId: string;
  userId: string;
}) {
  const { input, organizationId, userId } = params;
  await requireTankInOrg(input.tank_id, organizationId);

  const [created] = await db
    .insert(lab_oil_analyses)
    .values({
      tank_id: input.tank_id,
      sample_type: input.sample_type,
      collected_at: input.collected_at,
      issued_at: input.issued_at,
      certificate_number: input.certificate_number,
      laboratory_name: input.laboratory_name,
      method_density: input.method_density ?? null,
      method_basic_sediment_water: input.method_basic_sediment_water ?? null,
      density_at_20c: input.density_at_20c,
      water_and_sediment_percent: input.water_and_sediment_percent,
      salinity: input.salinity ?? null,
      organization_id: organizationId,
      created_by_user_id: userId,
    })
    .returning();
  if (!created) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao criar análise de laboratório",
    });
  }
  return schema.v1.create.output.parse(created);
}

type UpdateLabOilAnalysisInput = z.infer<typeof schema.v1.update.input>;

export async function updateLabOilAnalysis(params: {
  input: UpdateLabOilAnalysisInput;
  ability: AppAbility;
}) {
  const { id, ...data } = params.input;
  const [existing] = await db
    .select()
    .from(lab_oil_analyses)
    .where(eq(lab_oil_analyses.id, id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Análise de laboratório não encontrada",
    });
  }
  if (params.ability.cannot("update", subject("LabOilAnalyses", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message:
        "Você não tem permissão para atualizar esta análise de laboratório",
    });
  }

  const [updated] = await db
    .update(lab_oil_analyses)
    .set(data)
    .where(eq(lab_oil_analyses.id, id))
    .returning();
  if (!updated) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao atualizar análise de laboratório",
    });
  }
  return schema.v1.update.output.parse(updated);
}

type DeleteLabOilAnalysisInput = z.infer<typeof schema.v1.delete.input>;

export async function deleteLabOilAnalysis(params: {
  input: DeleteLabOilAnalysisInput;
  ability: AppAbility;
}) {
  const [existing] = await db
    .select()
    .from(lab_oil_analyses)
    .where(eq(lab_oil_analyses.id, params.input.id));
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Análise de laboratório não encontrada",
    });
  }
  if (params.ability.cannot("delete", subject("LabOilAnalyses", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message:
        "Você não tem permissão para excluir esta análise de laboratório",
    });
  }

  await db
    .delete(lab_oil_analyses)
    .where(eq(lab_oil_analyses.id, params.input.id));
  return null;
}
