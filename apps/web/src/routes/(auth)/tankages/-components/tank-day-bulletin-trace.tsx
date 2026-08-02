import type {
  TankDayBulletinAuditEventOutput,
  TankDayBulletinOutput,
} from "@lindaflor/shared/schemas/tankage/day-bulletins";
import { Loader2 } from "lucide-react";

import { Card, CardContent } from "@/components/ui/card";
import { Time } from "@/components/ui/time";
import {
  buildBulletinTraceDetail,
  buildBulletinTraceLabel,
} from "@/routes/(auth)/tankages/-components/tank-day-bulletin-trace-labels";

interface TankDayBulletinTraceProps {
  bulletin: TankDayBulletinOutput;
  events: TankDayBulletinAuditEventOutput[];
  isPending: boolean;
}

type TraceEntry = {
  key: string;
  at: Date;
  label: string;
  detail: string | null;
};

function buildTraceEntries(
  events: TankDayBulletinAuditEventOutput[],
): TraceEntry[] {
  return events.map((event) => ({
    key: event.id,
    at: event.occurred_at,
    label: buildBulletinTraceLabel(event),
    detail: buildBulletinTraceDetail(event),
  }));
}

export function TankDayBulletinTrace({
  bulletin,
  events,
  isPending,
}: TankDayBulletinTraceProps) {
  const entries = buildTraceEntries(events);
  const statusLabel =
    bulletin.status === "approved"
      ? "Boletim aprovado — medições bloqueadas"
      : bulletin.reopened_at != null
        ? "Boletim aberto após reabertura"
        : "Boletim aberto para edição";

  if (entries.length === 0) {
    return null;
  }

  return (
    <Card className="order-2 gap-0 py-0 shadow-sm">
      <CardContent className="p-4">
        <div className="flex flex-col gap-3">
          <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
            Histórico do boletim
          </p>
          <p className="text-sm text-foreground">{statusLabel}</p>
          {isPending ? (
            <div className="flex justify-center py-2 text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
            </div>
          ) : (
            <ul className="flex max-h-64 flex-col gap-2 overflow-y-auto text-sm">
              {entries.map((entry) => (
                <li
                  key={entry.key}
                  className="flex flex-col gap-0.5 border-l-2 border-muted-foreground/30 pl-3"
                >
                  <span>{entry.label}</span>
                  {entry.detail != null ? (
                    <span className="text-xs text-muted-foreground">
                      {entry.detail}
                    </span>
                  ) : null}
                  <span className="text-xs text-muted-foreground tabular-nums">
                    <Time date={entry.at} formatStr="dd/MM/yyyy HH:mm" />
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
