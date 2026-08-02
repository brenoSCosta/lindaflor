import type { TankSnapshotOutput } from "@lindaflor/shared/schemas/tankage/tanks";
import { Cylinder } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import {
  formatVolumeM3,
  formatVolumeOilBarrels,
} from "@/routes/(auth)/arqueacao/-components/calibration-status";
import {
  AnimatedTankGauge,
  AnimatedTankPercent,
  type TankFillTone,
} from "@/routes/(auth)/tankages/-components/animated-tank-gauge";
import { formatTankHeightAtualMax } from "@/routes/(auth)/tankages/-components/tank-height-display";

function calibrationBadge(status: TankSnapshotOutput["calibration_status"]) {
  switch (status) {
    case "current":
      return {
        label: "Arqueação vigente",
        variant: "outline" as const,
        className: "border-success/40 bg-success/10 text-success",
      };
    case "expired":
      return {
        label: "Arqueação expirada",
        variant: "destructive" as const,
        className: "",
      };
    case "future":
      return {
        label: "Arqueação futura",
        variant: "secondary" as const,
        className: "",
      };
    default:
      return {
        label: "Sem arqueação vigente",
        variant: "outline" as const,
        className: "",
      };
  }
}

function stockVolumeM3(snapshot: TankSnapshotOutput): number | null {
  return (
    snapshot.current_net_oil_volume_m3_20c ?? snapshot.current_volume_m3 ?? null
  );
}

function stockFillPercent(snapshot: TankSnapshotOutput): number | null {
  if (
    snapshot.capacity_height_m == null ||
    snapshot.capacity_height_m <= 0 ||
    snapshot.current_height_m == null
  ) {
    return null;
  }
  return Math.min(
    100,
    Math.round((snapshot.current_height_m / snapshot.capacity_height_m) * 100),
  );
}

/** Fill-level tone: room to spare → approaching capacity → near/full. */
function stockFillTone(fillPercent: number | null): TankFillTone {
  if (fillPercent == null) return "primary";
  if (fillPercent >= 90) return "destructive";
  if (fillPercent >= 70) return "warning";
  return "primary";
}

const fillToneTextClassName = {
  primary: "text-primary",
  warning: "text-warning",
  destructive: "text-destructive",
} as const;

type TankSnapshotCardProps = {
  snapshot: TankSnapshotOutput;
  layout?: "inline" | "dashboard" | "compact";
  /** Latest oil temperature (°C) for the mobile monitoring strip. */
  latestOilTemperatureC?: number | null;
};

export function TankSnapshotCard({
  snapshot,
  layout = "inline",
  latestOilTemperatureC = null,
}: TankSnapshotCardProps) {
  const badge = calibrationBadge(snapshot.calibration_status);
  const fillPercent = stockFillPercent(snapshot);
  const isDashboard = layout === "dashboard";
  const isCompact = layout === "compact";
  const heightAtualMax = formatTankHeightAtualMax(snapshot);
  const stockM3 = stockVolumeM3(snapshot);
  const stockText = stockM3 != null ? formatVolumeM3(stockM3) : "—";
  const barrelsText =
    snapshot.current_volume_oil_barrels != null
      ? formatVolumeOilBarrels(snapshot.current_volume_oil_barrels)
      : null;
  const availableM3 =
    snapshot.capacity_volume_m3 != null && stockM3 != null
      ? Math.max(0, snapshot.capacity_volume_m3 - stockM3)
      : null;

  if (isDashboard) {
    const tone = stockFillTone(fillPercent);
    return (
      <div className="flex flex-col gap-3">
        <DashboardMobileMonitoring
          fillPercent={fillPercent}
          tone={tone}
          heightAtualMax={heightAtualMax}
          availableM3={availableM3}
          latestOilTemperatureC={latestOilTemperatureC}
        />
        <DashboardDesktopStrip
          snapshot={snapshot}
          badge={badge}
          stockText={stockText}
          barrelsText={barrelsText}
        />
        <DashboardDesktopTank
          fillPercent={fillPercent}
          tone={tone}
          heightAtualMax={heightAtualMax}
          availableM3={availableM3}
          latestOilTemperatureC={latestOilTemperatureC}
        />
      </div>
    );
  }

  return (
    <Card>
      <CardHeader className={cn(isCompact && "pb-3")}>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <div className="flex size-11 shrink-0 items-center justify-center rounded-lg bg-muted">
              <Cylinder className="size-5 text-muted-foreground" />
            </div>
            <div className="min-w-0 flex flex-col gap-1">
              <CardTitle className="text-xl font-semibold tracking-tight">
                {snapshot.tag}
              </CardTitle>
              <CardDescription className="truncate">
                {snapshot.installation_name} — {snapshot.concession_name}
              </CardDescription>
              {!isCompact ? (
                <p className="text-xs text-muted-foreground">
                  Trena: {snapshot.measurement_equipment_code ?? "—"}
                  {snapshot.latitude != null && snapshot.longitude != null
                    ? ` · ${snapshot.latitude.toFixed(5)}, ${snapshot.longitude.toFixed(5)}`
                    : null}
                </p>
              ) : null}
            </div>
          </div>
          <div className="flex flex-wrap items-end gap-4 sm:gap-6">
            <SnapshotMetric
              label="Capacidade"
              value={
                snapshot.capacity_volume_m3 != null
                  ? formatVolumeM3(snapshot.capacity_volume_m3)
                  : "—"
              }
            />
            <TankLevelColumn snapshot={snapshot} fillPercent={fillPercent} />
            <SnapshotMetric label="Estoque (m³)" value={stockText} />
            <div className="flex flex-col gap-1">
              <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
                Arqueação
              </p>
              <Badge
                variant={badge.variant}
                className={cn("font-normal", badge.className)}
              >
                {badge.label}
              </Badge>
            </div>
          </div>
        </div>
      </CardHeader>

      {isCompact ? (
        <CardContent className="border-t pt-3 text-xs text-muted-foreground">
          <p>Trena: {snapshot.measurement_equipment_code ?? "—"}</p>
          <p className="mt-1 tabular-nums">{heightAtualMax}</p>
        </CardContent>
      ) : null}
    </Card>
  );
}

function DashboardMobileMonitoring({
  fillPercent,
  tone,
  heightAtualMax,
  availableM3,
  latestOilTemperatureC,
}: {
  fillPercent: number | null;
  tone: TankFillTone;
  heightAtualMax: string;
  availableM3: number | null;
  latestOilTemperatureC: number | null;
}) {
  return (
    <Card className="gap-0 overflow-hidden py-0 shadow-sm lg:hidden">
      <CardContent className="relative flex gap-5 p-4">
        <AnimatedTankGauge fillPercent={fillPercent} tone={tone} size="sm" />
        <div className="min-w-0 flex-1">
          <p className="mb-1 text-[11px] font-bold tracking-wider text-muted-foreground uppercase">
            Status de volume
          </p>
          <AnimatedTankPercent
            value={fillPercent}
            className={cn(
              "text-3xl font-semibold tracking-tight tabular-nums",
              fillPercent != null
                ? fillToneTextClassName[tone]
                : "text-muted-foreground",
            )}
            suffixClassName="ml-1 text-lg font-normal text-muted-foreground"
          />
          <p className="mt-1 font-mono text-xs text-muted-foreground tabular-nums">
            {heightAtualMax}
          </p>
          <div className="mt-4 grid grid-cols-2 gap-4 border-t pt-3">
            <div>
              <span className="mb-0.5 block text-[10px] font-bold tracking-wider text-muted-foreground uppercase">
                Disponível
              </span>
              <span className="font-mono text-sm text-primary tabular-nums">
                {availableM3 != null ? formatVolumeM3(availableM3) : "—"}
              </span>
            </div>
            <div>
              <span className="mb-0.5 block text-[10px] font-bold tracking-wider text-muted-foreground uppercase">
                Temp. óleo
              </span>
              <span className="font-mono text-sm tabular-nums">
                {latestOilTemperatureC != null
                  ? `${latestOilTemperatureC.toFixed(1)} °C`
                  : "—"}
              </span>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function DashboardDesktopStrip({
  snapshot,
  badge,
  stockText,
  barrelsText,
}: {
  snapshot: TankSnapshotOutput;
  badge: ReturnType<typeof calibrationBadge>;
  stockText: string;
  barrelsText: string | null;
}) {
  return (
    <div className="hidden gap-4 lg:grid lg:grid-cols-12">
      <Card className="gap-0 py-0 shadow-sm lg:col-span-8">
        <CardContent className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <div className="flex size-12 shrink-0 items-center justify-center rounded-lg bg-primary/10">
              <Cylinder className="size-6 text-primary" />
            </div>
            <div className="flex min-w-0 flex-col gap-0.5">
              <p className="text-lg font-semibold tracking-tight">
                {snapshot.tag}
              </p>
              <p className="text-sm text-muted-foreground">
                {snapshot.installation_name} — {snapshot.concession_name}
              </p>
              <p className="font-mono text-xs text-muted-foreground tabular-nums">
                Trena: {snapshot.measurement_equipment_code ?? "—"}
              </p>
            </div>
          </div>
          <div className="flex flex-col gap-1 sm:items-end">
            <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
              Arqueação
            </p>
            <Badge
              variant={badge.variant}
              className={cn("font-normal", badge.className)}
            >
              {badge.label}
            </Badge>
          </div>
        </CardContent>
      </Card>

      <Card className="gap-0 py-0 shadow-sm lg:col-span-4">
        <CardContent className="flex h-full items-center justify-between gap-6 p-4">
          <SnapshotMetric
            label="Capacidade total"
            value={
              snapshot.capacity_volume_m3 != null
                ? formatVolumeM3(snapshot.capacity_volume_m3)
                : "—"
            }
          />
          <div className="flex flex-col gap-1 text-right">
            <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
              Estoque atual
            </p>
            <p className="text-lg font-semibold text-primary tabular-nums">
              {stockText}
            </p>
            {barrelsText != null ? (
              <p className="font-mono text-xs text-muted-foreground tabular-nums">
                {barrelsText}
              </p>
            ) : null}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function DashboardDesktopTank({
  fillPercent,
  tone,
  heightAtualMax,
  availableM3,
  latestOilTemperatureC,
}: {
  fillPercent: number | null;
  tone: TankFillTone;
  heightAtualMax: string;
  availableM3: number | null;
  latestOilTemperatureC: number | null;
}) {
  return (
    <Card className="hidden gap-0 overflow-hidden py-0 shadow-sm lg:block">
      <CardContent className="flex items-center gap-8 p-5">
        <AnimatedTankGauge fillPercent={fillPercent} tone={tone} size="md" />
        <div className="flex min-w-0 flex-1 flex-col gap-4">
          <div>
            <p className="mb-0.5 text-[11px] font-bold tracking-wider text-muted-foreground uppercase">
              Status de volume
            </p>
            <AnimatedTankPercent
              value={fillPercent}
              className={cn(
                "text-5xl font-semibold tracking-tight tabular-nums",
                fillPercent != null
                  ? fillToneTextClassName[tone]
                  : "text-muted-foreground",
              )}
              suffixClassName="ml-1 text-2xl font-normal text-muted-foreground"
            />
          </div>
          <div className="grid grid-cols-3 gap-4 border-t pt-4">
            <DesktopTankMetric
              label="Nível atual / máx"
              value={heightAtualMax}
            />
            <DesktopTankMetric
              label="Disponível"
              value={availableM3 != null ? formatVolumeM3(availableM3) : "—"}
              valueClassName="text-primary"
            />
            <DesktopTankMetric
              label="Temp. óleo"
              value={
                latestOilTemperatureC != null
                  ? `${latestOilTemperatureC.toFixed(1)} °C`
                  : "—"
              }
            />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function DesktopTankMetric({
  label,
  value,
  valueClassName,
}: {
  label: string;
  value: string;
  valueClassName?: string;
}) {
  return (
    <div className="min-w-0">
      <span className="mb-0.5 block text-[10px] font-bold tracking-wider text-muted-foreground uppercase">
        {label}
      </span>
      <span
        className={cn(
          "font-mono text-sm tabular-nums",
          valueClassName ?? "text-foreground",
        )}
      >
        {value}
      </span>
    </div>
  );
}

function TankLevelColumn({
  snapshot,
  fillPercent,
}: {
  snapshot: TankSnapshotOutput;
  fillPercent: number | null;
}) {
  const currentText =
    snapshot.current_height_m != null
      ? snapshot.current_height_m.toFixed(3)
      : "—";
  const maxText =
    snapshot.capacity_height_m != null
      ? snapshot.capacity_height_m.toFixed(3)
      : "—";

  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
        Nível
      </p>
      <p className="text-lg font-semibold tabular-nums">
        {currentText} / {maxText}{" "}
        <span className="text-sm font-medium text-muted-foreground">m</span>
      </p>
      {fillPercent != null ? (
        <div className="flex flex-col gap-1.5">
          <Progress value={fillPercent} className="gap-1.5">
            <span className="sr-only">{fillPercent}% da capacidade</span>
          </Progress>
          <p className="text-xs text-muted-foreground tabular-nums">
            {fillPercent}% da capacidade
          </p>
        </div>
      ) : null}
    </div>
  );
}

function SnapshotMetric({
  label,
  value,
  hint,
}: {
  label: string;
  value: string;
  hint?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <p className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
        {label}
      </p>
      <p className="text-lg font-semibold tabular-nums">{value}</p>
      {hint ? (
        <p className="text-xs text-muted-foreground tabular-nums">{hint}</p>
      ) : null}
    </div>
  );
}
