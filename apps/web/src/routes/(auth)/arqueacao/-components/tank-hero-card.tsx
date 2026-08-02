import type { TankOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import { Cylinder } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";
import type { CalibrationDisplayBadge } from "@/routes/(auth)/arqueacao/-components/calibration-status";
import { formatVolumeM3 } from "@/routes/(auth)/arqueacao/-components/calibration-status";

export function TankHeroCard({
  tank,
  maxVolume,
  heroStatus,
}: {
  tank: TankOutput;
  maxVolume: number | null;
  heroStatus: CalibrationDisplayBadge | null;
}) {
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <div className="flex size-11 shrink-0 items-center justify-center rounded-lg bg-muted">
              <Cylinder className="size-5 text-muted-foreground" />
            </div>
            <div className="min-w-0 space-y-1">
              <CardTitle className="text-xl font-semibold tracking-tight">
                {tank.tag}
              </CardTitle>
              <CardDescription className="truncate">
                {tank.installation_name}
                {tank.concession_name ? ` — ${tank.concession_name}` : ""}
              </CardDescription>
            </div>
          </div>
          <div className="flex flex-wrap items-end gap-4 sm:gap-6">
            <div className="space-y-1">
              <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
                Capacidade total
              </p>
              <p className="text-lg font-semibold tabular-nums">
                {maxVolume != null ? formatVolumeM3(maxVolume) : "—"}
              </p>
            </div>
            <div className="space-y-1">
              <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
                Status atual
              </p>
              {heroStatus ? (
                <Badge
                  variant={heroStatus.variant}
                  className={cn("font-normal", heroStatus.className)}
                >
                  {heroStatus.label === "Ativo" ? "Vigente" : heroStatus.label}
                </Badge>
              ) : (
                <Badge variant="outline" className="font-normal">
                  Sem arqueação vigente
                </Badge>
              )}
            </div>
          </div>
        </div>
      </CardHeader>
    </Card>
  );
}
