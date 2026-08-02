import { db } from "@lindaflor/db";
import type { AuditEventInsert } from "@lindaflor/db/schema/audit";
import { audit_events } from "@lindaflor/db/schema/audit";

type AuditExecutor = Pick<typeof db, "insert">;

export function tankDayAggregateId(
  tankId: string,
  operationalDay: string,
): string {
  return `${tankId}:${operationalDay}`;
}

export async function recordAuditEvents(
  executor: AuditExecutor,
  events: AuditEventInsert[],
): Promise<void> {
  if (events.length === 0) return;
  await executor.insert(audit_events).values(events);
}
