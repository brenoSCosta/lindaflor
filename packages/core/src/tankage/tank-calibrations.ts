import { db } from "@lindaflor/db";
import {
  tank_calibration_points,
  tank_calibrations,
  tanks,
} from "@lindaflor/db/schema/tankage";
import {
  dateRangesOverlap,
  interpolateVolume,
  planOpenEndedClosures,
} from "@lindaflor/shared/functions/tankage/calibration-volume";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/tankage";
import { ORPCError } from "@orpc/server";
import {
  and,
  asc,
  eq,
  gte,
  inArray,
  isNull,
  lte,
  ne,
  or,
  sql,
} from "drizzle-orm";
import { v7 as uuidv7 } from "uuid";
import type { z } from "zod";

const calibrationColumns = {
  id: tank_calibrations.id,
  tank_id: tank_calibrations.tank_id,
  certificate_number: tank_calibrations.certificate_number,
  issued_at: tank_calibrations.issued_at,
  valid_from: tank_calibrations.valid_from,
  valid_until: tank_calibrations.valid_until,
  organization_id: tank_calibrations.organization_id,
  created_by_user_id: tank_calibrations.created_by_user_id,
  created_at: tank_calibrations.created_at,
  updated_at: tank_calibrations.updated_at,
};

type ValidityRange = {
  id?: string;
  valid_from: string;
  valid_until: string | null;
};

async function loadDetail(id: string) {
  const [calibration] = await db
    .select(calibrationColumns)
    .from(tank_calibrations)
    .where(eq(tank_calibrations.id, id));
  if (!calibration) return null;
  const at = new Date().toISOString().slice(0, 10);
  const points = await db
    .select({
      id: tank_calibration_points.id,
      calibration_id: tank_calibration_points.calibration_id,
      height_cm: tank_calibration_points.height_cm,
      volume_m3: tank_calibration_points.volume_m3,
    })
    .from(tank_calibration_points)
    .where(eq(tank_calibration_points.calibration_id, id))
    .orderBy(asc(tank_calibration_points.height_cm));
  return {
    ...calibration,
    points,
    is_expired: calibration.valid_until != null && calibration.valid_until < at,
  };
}

function findOverlapConflict(
  rows: ValidityRange[],
  validFrom: string,
  validUntil: string | null,
): ValidityRange | undefined {
  return rows.find((row) =>
    dateRangesOverlap(validFrom, validUntil, row.valid_from, row.valid_until),
  );
}

function formatRange(row: ValidityRange): string {
  return `${row.valid_from}–${row.valid_until ?? "sem fim"}`;
}

async function assertTankInOrg(
  tankId: string,
  organizationId: string,
): Promise<typeof tanks.$inferSelect> {
  const [tank] = await db.select().from(tanks).where(eq(tanks.id, tankId));
  if (!tank || tank.organization_id !== organizationId) {
    throw new ORPCError("NOT_FOUND", { message: "Tanque não encontrado" });
  }
  return tank;
}

function sortPoints<T extends { height_cm: number }>(points: T[]): T[] {
  return points.toSorted((a, b) => a.height_cm - b.height_cm);
}

function isValidOnDate(at: string) {
  return and(
    lte(tank_calibrations.valid_from, at),
    or(
      isNull(tank_calibrations.valid_until),
      gte(tank_calibrations.valid_until, at),
    ),
  );
}

export async function listCurrentCalibrations(args: {
  input: { at?: string };
  organizationId: string;
}) {
  const at = args.input.at ?? new Date().toISOString().slice(0, 10);
  const rows = await db
    .select({
      tank_id: tank_calibrations.tank_id,
      certificate_number: tank_calibrations.certificate_number,
      valid_until: tank_calibrations.valid_until,
    })
    .from(tank_calibrations)
    .where(
      and(
        eq(tank_calibrations.organization_id, args.organizationId),
        isValidOnDate(at),
        sql`exists (
            select 1
            from ${tank_calibration_points}
            where ${tank_calibration_points.calibration_id} = ${tank_calibrations.id}
          )`,
      ),
    )
    .orderBy(asc(tank_calibrations.tank_id));

  return schema.v1.calibration.list.current.output.parse({
    data: rows,
  });
}

export async function listCalibrationsByTank(args: {
  input: { tank_id: string };
  organizationId: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);

  const at = new Date().toISOString().slice(0, 10);
  const rows = await db
    .select({
      ...calibrationColumns,
      points_count: sql<number>`(
          select count(*)::int
          from ${tank_calibration_points}
          where ${tank_calibration_points.calibration_id} = ${tank_calibrations.id}
        )`.mapWith(Number),
      is_expired: sql<boolean>`(
          ${tank_calibrations.valid_until} is not null
          and ${tank_calibrations.valid_until} < ${at}
        )`.mapWith(Boolean),
    })
    .from(tank_calibrations)
    .where(
      and(
        eq(tank_calibrations.tank_id, args.input.tank_id),
        eq(tank_calibrations.organization_id, args.organizationId),
      ),
    )
    .orderBy(asc(tank_calibrations.valid_from));

  return schema.v1.calibration.listBy.tank.output.parse({
    data: rows,
  });
}

export async function getCalibrationById(args: {
  input: { id: string };
  ability: AppAbility;
}) {
  const detail = await loadDetail(args.input.id);
  if (!detail) {
    throw new ORPCError("NOT_FOUND", {
      message: "Certificado de arqueação não encontrado",
    });
  }
  if (args.ability.cannot("read", subject("TankCalibrations", detail))) {
    throw new ORPCError("FORBIDDEN", {
      message: "Você não tem permissão para ler este certificado de arqueação",
    });
  }
  return schema.v1.calibration.getBy.id.output.parse(detail);
}

export async function createCalibration(args: {
  input: z.infer<typeof schema.v1.calibration.create.input>;
  organizationId: string;
  createdByUserId: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);

  const points = sortPoints(args.input.points ?? []);
  const validUntil = args.input.valid_until ?? null;
  const created = await db.transaction(async (tx) => {
    await tx
      .select({ id: tanks.id })
      .from(tanks)
      .where(eq(tanks.id, args.input.tank_id))
      .for("update");

    const existingRanges = await tx
      .select({
        id: tank_calibrations.id,
        valid_from: tank_calibrations.valid_from,
        valid_until: tank_calibrations.valid_until,
      })
      .from(tank_calibrations)
      .where(eq(tank_calibrations.tank_id, args.input.tank_id));

    const { closes, conflict } = planOpenEndedClosures(
      existingRanges,
      args.input.valid_from,
      validUntil,
    );
    if (conflict) {
      throw new ORPCError("CONFLICT", {
        message: `Vigência sobrepõe certificado existente (${formatRange(conflict)})`,
      });
    }
    if (closes.length > 0) {
      await Promise.all(
        closes.map((close) =>
          tx
            .update(tank_calibrations)
            .set({ valid_until: close.valid_until })
            .where(eq(tank_calibrations.id, close.id)),
        ),
      );
    }

    const [row] = await tx
      .insert(tank_calibrations)
      .values({
        tank_id: args.input.tank_id,
        certificate_number: args.input.certificate_number,
        issued_at: args.input.issued_at ?? null,
        valid_from: args.input.valid_from,
        valid_until: validUntil,
        organization_id: args.organizationId,
        created_by_user_id: args.createdByUserId,
      })
      .returning();
    if (!row) {
      throw new ORPCError("INTERNAL_SERVER_ERROR", {
        message: "Falha ao criar certificado de arqueação",
      });
    }
    if (points.length > 0) {
      await tx.insert(tank_calibration_points).values(
        points.map((point) => ({
          id: uuidv7(),
          calibration_id: row.id,
          height_cm: point.height_cm,
          volume_m3: point.volume_m3,
        })),
      );
    }
    return row;
  });

  const detail = await loadDetail(created.id);
  if (!detail) {
    throw new ORPCError("INTERNAL_SERVER_ERROR", {
      message: "Falha ao carregar certificado criado",
    });
  }
  return schema.v1.calibration.create.output.parse(detail);
}

export async function updateCalibration(args: {
  input: z.infer<typeof schema.v1.calibration.update.input>;
  ability: AppAbility;
}) {
  const existing = await loadDetail(args.input.id);
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Certificado de arqueação não encontrado",
    });
  }
  if (args.ability.cannot("update", subject("TankCalibrations", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message: existing.is_expired
        ? "Certificado expirado não pode ser alterado"
        : "Você não tem permissão para atualizar este certificado",
    });
  }

  const validFrom = args.input.valid_from ?? existing.valid_from;
  const validUntil =
    args.input.valid_until === undefined
      ? existing.valid_until
      : args.input.valid_until;
  if (validUntil != null && validUntil < validFrom) {
    throw new ORPCError("BAD_REQUEST", {
      message:
        "Validade final deve ser igual ou posterior ao início da vigência",
    });
  }

  await db.transaction(async (tx) => {
    await tx
      .select({ id: tanks.id })
      .from(tanks)
      .where(eq(tanks.id, existing.tank_id))
      .for("update");

    const existingRanges = await tx
      .select({
        id: tank_calibrations.id,
        valid_from: tank_calibrations.valid_from,
        valid_until: tank_calibrations.valid_until,
      })
      .from(tank_calibrations)
      .where(
        and(
          eq(tank_calibrations.tank_id, existing.tank_id),
          ne(tank_calibrations.id, existing.id),
        ),
      );

    const conflict = findOverlapConflict(existingRanges, validFrom, validUntil);
    if (conflict) {
      throw new ORPCError("CONFLICT", {
        message: `Vigência sobrepõe certificado existente (${formatRange(conflict)})`,
      });
    }

    await tx
      .update(tank_calibrations)
      .set({
        certificate_number:
          args.input.certificate_number ?? existing.certificate_number,
        issued_at:
          args.input.issued_at === undefined
            ? existing.issued_at
            : args.input.issued_at,
        valid_from: validFrom,
        valid_until: validUntil,
      })
      .where(eq(tank_calibrations.id, args.input.id));
  });

  const detail = await loadDetail(args.input.id);
  if (!detail) {
    throw new ORPCError("NOT_FOUND", {
      message: "Certificado de arqueação não encontrado",
    });
  }
  return schema.v1.calibration.update.output.parse(detail);
}

export async function replaceCalibrationPoints(args: {
  input: z.infer<typeof schema.v1.calibration.replace.point.input>;
  ability: AppAbility;
}) {
  const existing = await loadDetail(args.input.id);
  if (!existing) {
    throw new ORPCError("NOT_FOUND", {
      message: "Certificado de arqueação não encontrado",
    });
  }
  if (args.ability.cannot("update", subject("TankCalibrations", existing))) {
    throw new ORPCError("FORBIDDEN", {
      message: existing.is_expired
        ? "Certificado expirado não pode ser alterado"
        : "Você não tem permissão para atualizar este certificado",
    });
  }

  const points = sortPoints(args.input.points);
  await db.transaction(async (tx) => {
    await tx
      .delete(tank_calibration_points)
      .where(eq(tank_calibration_points.calibration_id, args.input.id));
    await tx.insert(tank_calibration_points).values(
      points.map((point) => ({
        id: uuidv7(),
        calibration_id: args.input.id,
        height_cm: point.height_cm,
        volume_m3: point.volume_m3,
      })),
    );
  });

  const detail = await loadDetail(args.input.id);
  if (!detail) {
    throw new ORPCError("NOT_FOUND", {
      message: "Certificado de arqueação não encontrado",
    });
  }
  return schema.v1.calibration.replace.point.output.parse(detail);
}

export async function deleteCalibrations(args: {
  input: { ids: string[] };
  ability: AppAbility;
}) {
  const rows = await db
    .select(calibrationColumns)
    .from(tank_calibrations)
    .where(inArray(tank_calibrations.id, args.input.ids));
  if (rows.length === 0) {
    throw new ORPCError("NOT_FOUND", {
      message: "Nenhum certificado de arqueação encontrado",
    });
  }
  const notFoundIds = args.input.ids.filter(
    (id) => !rows.some((r) => r.id === id),
  );
  if (notFoundIds.length > 0) {
    throw new ORPCError("NOT_FOUND", {
      message: `Certificados não encontrados: ${notFoundIds.join(", ")}`,
    });
  }
  const at = new Date().toISOString().slice(0, 10);
  const unauthorized = rows.filter((r) =>
    args.ability.cannot(
      "delete",
      subject("TankCalibrations", {
        ...r,
        is_expired: r.valid_until != null && r.valid_until < at,
      }),
    ),
  );
  if (unauthorized.length > 0) {
    const expiredBlocked = unauthorized.some(
      (r) => r.valid_until != null && r.valid_until < at,
    );
    throw new ORPCError("FORBIDDEN", {
      message: expiredBlocked
        ? "Certificado expirado não pode ser excluído"
        : "Você não tem permissão para excluir um ou mais destes certificados",
    });
  }
  await db
    .delete(tank_calibrations)
    .where(inArray(tank_calibrations.id, args.input.ids));
  return null;
}

export async function resolveCalibrationVolume(args: {
  input: { tank_id: string; at: string; height_m: number };
  organizationId: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);

  const [calibration] = await db
    .select(calibrationColumns)
    .from(tank_calibrations)
    .where(
      and(
        eq(tank_calibrations.tank_id, args.input.tank_id),
        eq(tank_calibrations.organization_id, args.organizationId),
        isValidOnDate(args.input.at),
      ),
    )
    .orderBy(asc(tank_calibrations.valid_from))
    .limit(1);

  if (!calibration) {
    throw new ORPCError("NOT_FOUND", {
      message: `Nenhum certificado de arqueação vigente em ${args.input.at}`,
    });
  }

  const points = await db
    .select({
      height_cm: tank_calibration_points.height_cm,
      volume_m3: tank_calibration_points.volume_m3,
    })
    .from(tank_calibration_points)
    .where(eq(tank_calibration_points.calibration_id, calibration.id))
    .orderBy(asc(tank_calibration_points.height_cm));

  const heightCm = args.input.height_m * 100;
  const result = interpolateVolume(points, heightCm);
  if (!result.ok) {
    throw new ORPCError("BAD_REQUEST", { message: result.message });
  }
  return schema.v1.calibration.resolve.volume.output.parse({
    volume_m3: result.volume_m3,
    calibration_id: calibration.id,
    interpolated: result.interpolated,
  });
}
