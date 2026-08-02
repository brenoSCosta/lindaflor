import {
  recordAuditEvents,
  tankDayAggregateId,
} from "@lindaflor/core/lib/audit/record";
import { db } from "@lindaflor/db";
import { audit_events } from "@lindaflor/db/schema/audit";
import { users } from "@lindaflor/db/schema/auth";
import {
  tank_day_bulletins,
  tankages,
  tanks,
} from "@lindaflor/db/schema/tankage";
import type { AppAbility } from "@lindaflor/shared/lib/ability/subjects";
import { subject } from "@lindaflor/shared/lib/ability/subjects";
import { schema } from "@lindaflor/shared/schemas/tankage";
import { ORPCError } from "@orpc/server";
import { and, desc, eq, sql } from "drizzle-orm";
import { alias } from "drizzle-orm/pg-core";
import { v7 as uuidv7 } from "uuid";

const approvedByUser = alias(users, "tank_day_bulletin_approved_by");
const reopenedByUser = alias(users, "tank_day_bulletin_reopened_by");

const bulletinColumns = {
  id: tank_day_bulletins.id,
  tank_id: tank_day_bulletins.tank_id,
  operational_day: tank_day_bulletins.operational_day,
  status: tank_day_bulletins.status,
  approved_at: tank_day_bulletins.approved_at,
  approved_by_user_id: tank_day_bulletins.approved_by_user_id,
  reopened_at: tank_day_bulletins.reopened_at,
  reopened_by_user_id: tank_day_bulletins.reopened_by_user_id,
  organization_id: tank_day_bulletins.organization_id,
  approved_by_name: approvedByUser.name,
  reopened_by_name: reopenedByUser.name,
};

async function assertTankInOrg(tankId: string, organizationId: string) {
  const [tank] = await db
    .select()
    .from(tanks)
    .where(
      and(eq(tanks.id, tankId), eq(tanks.organization_id, organizationId)),
    );
  if (!tank) {
    throw new ORPCError("NOT_FOUND", { message: "Tanque não encontrado" });
  }
  return tank;
}

async function loadBulletinOutput(args: {
  organizationId: string;
  tankId: string;
  operationalDay: string;
}) {
  const [row] = await db
    .select(bulletinColumns)
    .from(tank_day_bulletins)
    .leftJoin(
      approvedByUser,
      eq(tank_day_bulletins.approved_by_user_id, approvedByUser.id),
    )
    .leftJoin(
      reopenedByUser,
      eq(tank_day_bulletins.reopened_by_user_id, reopenedByUser.id),
    )
    .where(
      and(
        eq(tank_day_bulletins.organization_id, args.organizationId),
        eq(tank_day_bulletins.tank_id, args.tankId),
        eq(tank_day_bulletins.operational_day, args.operationalDay),
      ),
    )
    .limit(1);

  if (row) {
    return schema.v1.bulletin.getBy.day.output.parse(row);
  }

  return schema.v1.bulletin.getBy.day.output.parse({
    id: null,
    tank_id: args.tankId,
    operational_day: args.operationalDay,
    status: "open",
    approved_at: null,
    approved_by_user_id: null,
    approved_by_name: null,
    reopened_at: null,
    reopened_by_user_id: null,
    reopened_by_name: null,
    organization_id: args.organizationId,
  });
}

export async function getTankDayBulletinByDay(args: {
  input: { tank_id: string; operational_day: string };
  organizationId: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);
  return loadBulletinOutput({
    organizationId: args.organizationId,
    tankId: args.input.tank_id,
    operationalDay: args.input.operational_day,
  });
}

export async function listTankDayBulletinEventsByDay(args: {
  input: { tank_id: string; operational_day: string };
  organizationId: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);
  const aggregateId = tankDayAggregateId(
    args.input.tank_id,
    args.input.operational_day,
  );
  const rows = await db
    .select({
      id: audit_events.id,
      entity_type: audit_events.entity_type,
      action: audit_events.action,
      entity_id: audit_events.entity_id,
      actor_user_id: audit_events.actor_user_id,
      actor_name: sql<string>`coalesce(${users.name}, ${audit_events.actor_name})`,
      occurred_at: audit_events.occurred_at,
      metadata: audit_events.metadata,
      aggregate_type: audit_events.aggregate_type,
    })
    .from(audit_events)
    .leftJoin(users, eq(audit_events.actor_user_id, users.id))
    .where(
      and(
        eq(audit_events.organization_id, args.organizationId),
        eq(audit_events.aggregate_type, "tank_day_bulletin"),
        eq(audit_events.aggregate_id, aggregateId),
      ),
    )
    .orderBy(desc(audit_events.occurred_at), desc(audit_events.id))
    .limit(200);

  return schema.v1.bulletin.event.listBy.day.output.parse({
    data: rows,
  });
}

export async function approveTankDayBulletin(args: {
  input: { tank_id: string; operational_day: string };
  organizationId: string;
  ability: AppAbility;
  actorUserId: string;
  actorName: string;
}) {
  const tank = await assertTankInOrg(args.input.tank_id, args.organizationId);

  const bulletin = await loadBulletinOutput({
    organizationId: args.organizationId,
    tankId: args.input.tank_id,
    operationalDay: args.input.operational_day,
  });
  if (args.ability.cannot("approve", subject("TankDayBulletins", bulletin))) {
    throw new ORPCError("CONFLICT", {
      message: "Boletim já está aprovado",
    });
  }

  const now = new Date();

  await db.transaction(async (tx) => {
    const [existing] = await tx
      .select({ id: tank_day_bulletins.id })
      .from(tank_day_bulletins)
      .where(
        and(
          eq(tank_day_bulletins.organization_id, args.organizationId),
          eq(tank_day_bulletins.tank_id, args.input.tank_id),
          eq(tank_day_bulletins.operational_day, args.input.operational_day),
        ),
      )
      .limit(1);

    let bulletinId: string;
    if (existing) {
      bulletinId = existing.id;
      await tx
        .update(tank_day_bulletins)
        .set({
          status: "approved",
          approved_at: now,
          approved_by_user_id: args.actorUserId,
          reopened_at: null,
          reopened_by_user_id: null,
        })
        .where(eq(tank_day_bulletins.id, existing.id));
    } else {
      bulletinId = uuidv7();
      await tx.insert(tank_day_bulletins).values({
        id: bulletinId,
        tank_id: tank.id,
        operational_day: args.input.operational_day,
        status: "approved",
        approved_at: now,
        approved_by_user_id: args.actorUserId,
        organization_id: args.organizationId,
      });
    }

    await recordAuditEvents(tx, [
      {
        organization_id: args.organizationId,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(
          args.input.tank_id,
          args.input.operational_day,
        ),
        entity_type: "tank_day_bulletin",
        entity_id: bulletinId,
        action: "approve",
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: null,
        occurred_at: now,
      },
    ]);
  });

  return loadBulletinOutput({
    organizationId: args.organizationId,
    tankId: args.input.tank_id,
    operationalDay: args.input.operational_day,
  });
}

export async function reopenTankDayBulletin(args: {
  input: { tank_id: string; operational_day: string };
  organizationId: string;
  ability: AppAbility;
  actorUserId: string;
  actorName: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);

  const bulletin = await loadBulletinOutput({
    organizationId: args.organizationId,
    tankId: args.input.tank_id,
    operationalDay: args.input.operational_day,
  });
  if (args.ability.cannot("reopen", subject("TankDayBulletins", bulletin))) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Boletim não está aprovado",
    });
  }

  const [existing] = await db
    .select()
    .from(tank_day_bulletins)
    .where(
      and(
        eq(tank_day_bulletins.organization_id, args.organizationId),
        eq(tank_day_bulletins.tank_id, args.input.tank_id),
        eq(tank_day_bulletins.operational_day, args.input.operational_day),
      ),
    )
    .limit(1);

  if (!existing) {
    throw new ORPCError("BAD_REQUEST", {
      message: "Boletim não está aprovado",
    });
  }

  const now = new Date();

  await db.transaction(async (tx) => {
    await tx
      .update(tank_day_bulletins)
      .set({
        status: "open",
        reopened_at: now,
        reopened_by_user_id: args.actorUserId,
      })
      .where(eq(tank_day_bulletins.id, existing.id));

    await recordAuditEvents(tx, [
      {
        organization_id: args.organizationId,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(
          args.input.tank_id,
          args.input.operational_day,
        ),
        entity_type: "tank_day_bulletin",
        entity_id: existing.id,
        action: "reopen",
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: null,
        occurred_at: now,
      },
    ]);
  });

  return loadBulletinOutput({
    organizationId: args.organizationId,
    tankId: args.input.tank_id,
    operationalDay: args.input.operational_day,
  });
}

export async function deleteTankDayBulletinOperations(args: {
  input: { tank_id: string; operational_day: string };
  organizationId: string;
  ability: AppAbility;
  actorUserId: string;
  actorName: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);

  const bulletin = await loadBulletinOutput({
    organizationId: args.organizationId,
    tankId: args.input.tank_id,
    operationalDay: args.input.operational_day,
  });
  if (args.ability.cannot("delete", subject("TankDayBulletins", bulletin))) {
    throw new ORPCError("FORBIDDEN", {
      message:
        bulletin.status === "approved"
          ? "Boletim aprovado — não é possível alterar medições"
          : "Você não tem permissão para alterar medições",
    });
  }

  const now = new Date();

  await db.transaction(async (tx) => {
    const existingRows = await tx
      .select({ id: tankages.id })
      .from(tankages)
      .where(
        and(
          eq(tankages.organization_id, args.organizationId),
          eq(tankages.tank_id, args.input.tank_id),
          eq(tankages.operational_day, args.input.operational_day),
        ),
      );

    await tx
      .delete(tankages)
      .where(
        and(
          eq(tankages.organization_id, args.organizationId),
          eq(tankages.tank_id, args.input.tank_id),
          eq(tankages.operational_day, args.input.operational_day),
        ),
      );

    await recordAuditEvents(tx, [
      {
        organization_id: args.organizationId,
        aggregate_type: "tank_day_bulletin",
        aggregate_id: tankDayAggregateId(
          args.input.tank_id,
          args.input.operational_day,
        ),
        entity_type: "tank_day_bulletin",
        entity_id: bulletin.id,
        action: "delete",
        actor_user_id: args.actorUserId,
        actor_name: args.actorName,
        metadata: {
          deleted_count: existingRows.length,
          deleted_ids: existingRows.map((row) => row.id),
        },
        occurred_at: now,
      },
    ]);
  });

  return null;
}

export async function listTankDayBulletinsByTank(args: {
  input: { tank_id: string };
  organizationId: string;
}) {
  await assertTankInOrg(args.input.tank_id, args.organizationId);
  const rows = await db
    .select({
      operational_day: tank_day_bulletins.operational_day,
      status: tank_day_bulletins.status,
    })
    .from(tank_day_bulletins)
    .where(
      and(
        eq(tank_day_bulletins.organization_id, args.organizationId),
        eq(tank_day_bulletins.tank_id, args.input.tank_id),
      ),
    )
    .orderBy(desc(tank_day_bulletins.operational_day));

  return schema.v1.bulletin.listBy.tank.output.parse({ data: rows });
}
